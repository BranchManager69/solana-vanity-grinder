use actix_web::{web, App, HttpResponse, HttpServer, Responder, middleware, get, post};
use actix_cors::Cors;
use serde::{Serialize, Deserialize};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use tokio::sync::mpsc;
use log::{info, warn, error};
use uuid::Uuid;
use std::collections::HashMap;
use crate::cuda_helpers::CudaDevice;
use crate::cuda::vanity_generator::{self, VanityMode, VanityResult};
use std::time::{Duration, Instant};
use tokio::time::sleep;

// API Data structures
#[derive(Debug, Serialize, Deserialize)]
pub struct VanityRequest {
    pub pattern: String,
    pub is_suffix: bool,
    pub case_sensitive: bool,
    pub max_attempts: Option<u64>,
    pub callback_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VanityJob {
    pub id: String,
    pub status: JobStatus,
    pub request: VanityRequest,
    pub result: Option<VanityResponse>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub attempts: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VanityResponse {
    pub address: String,
    pub keypair_bytes: Vec<u8>,
    pub attempts: u64,
    pub duration_ms: u64,
    pub rate_per_second: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

// Job manager
#[derive(Debug)]
pub struct JobManager {
    jobs: Mutex<HashMap<String, VanityJob>>,
    cuda_device: Arc<Mutex<Option<CudaDevice>>>,
    stop_flag: Arc<AtomicBool>,
}

impl JobManager {
    pub fn new() -> Self {
        JobManager {
            jobs: Mutex::new(HashMap::new()),
            cuda_device: Arc::new(Mutex::new(None)),
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn init_cuda(&self) -> Result<(), String> {
        let mut device_guard = self.cuda_device.lock().unwrap();
        
        if device_guard.is_none() {
            match CudaDevice::new() {
                Ok(device) => {
                    info!("CUDA device initialized: {}", device.get_info());
                    *device_guard = Some(device);
                    Ok(())
                },
                Err(e) => Err(format!("Failed to initialize CUDA device: {}", e)),
            }
        } else {
            Ok(())
        }
    }

    pub fn add_job(&self, request: VanityRequest) -> Result<String, String> {
        // Generate a unique job ID
        let job_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        
        let job = VanityJob {
            id: job_id.clone(),
            status: JobStatus::Queued,
            request,
            result: None,
            created_at: now,
            updated_at: now,
            attempts: 0,
            duration_ms: 0,
        };
        
        let mut jobs = self.jobs.lock().unwrap();
        jobs.insert(job_id.clone(), job);
        
        Ok(job_id)
    }

    pub fn get_job(&self, job_id: &str) -> Option<VanityJob> {
        let jobs = self.jobs.lock().unwrap();
        jobs.get(job_id).cloned()
    }

    pub fn update_job_status(&self, job_id: &str, status: JobStatus) -> Result<(), String> {
        let mut jobs = self.jobs.lock().unwrap();
        
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = status;
            job.updated_at = chrono::Utc::now();
            Ok(())
        } else {
            Err(format!("Job not found: {}", job_id))
        }
    }

    pub fn update_job_attempts(&self, job_id: &str, attempts: u64) -> Result<(), String> {
        let mut jobs = self.jobs.lock().unwrap();
        
        if let Some(job) = jobs.get_mut(job_id) {
            job.attempts = attempts;
            job.updated_at = chrono::Utc::now();
            Ok(())
        } else {
            Err(format!("Job not found: {}", job_id))
        }
    }

    pub fn set_job_result(&self, job_id: &str, result: VanityResponse) -> Result<(), String> {
        let mut jobs = self.jobs.lock().unwrap();
        
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Completed;
            job.result = Some(result);
            job.updated_at = chrono::Utc::now();
            job.duration_ms = result.duration_ms;
            job.attempts = result.attempts;
            Ok(())
        } else {
            Err(format!("Job not found: {}", job_id))
        }
    }

    pub fn set_job_failed(&self, job_id: &str, error: &str) -> Result<(), String> {
        let mut jobs = self.jobs.lock().unwrap();
        
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Failed;
            job.updated_at = chrono::Utc::now();
            info!("Job {} failed: {}", job_id, error);
            Ok(())
        } else {
            Err(format!("Job not found: {}", job_id))
        }
    }

    pub fn cancel_job(&self, job_id: &str) -> Result<(), String> {
        let mut jobs = self.jobs.lock().unwrap();
        
        if let Some(job) = jobs.get_mut(job_id) {
            if job.status == JobStatus::Running || job.status == JobStatus::Queued {
                job.status = JobStatus::Cancelled;
                job.updated_at = chrono::Utc::now();
                self.stop_flag.store(true, Ordering::SeqCst);
                Ok(())
            } else {
                Err(format!("Job {} is in state {} and cannot be cancelled", job_id, job.status))
            }
        } else {
            Err(format!("Job not found: {}", job_id))
        }
    }

    pub fn list_jobs(&self) -> Vec<VanityJob> {
        let jobs = self.jobs.lock().unwrap();
        jobs.values().cloned().collect()
    }

    pub fn worker_loop(&self, job_id: String) -> Result<VanityResponse, String> {
        let job_option = self.get_job(&job_id);
        
        if job_option.is_none() {
            return Err(format!("Job not found: {}", job_id));
        }
        
        let job = job_option.unwrap();
        self.update_job_status(&job_id, JobStatus::Running)?;
        
        // Reset stop flag
        self.stop_flag.store(false, Ordering::SeqCst);
        
        // Get CUDA device
        let device_guard = self.cuda_device.lock().unwrap();
        let device = match &*device_guard {
            Some(d) => d,
            None => return Err("CUDA device not initialized".to_string()),
        };
        
        // Find optimal batch size
        let batch_size = match vanity_generator::find_optimal_batch_size(device) {
            Ok(size) => size,
            Err(e) => return Err(format!("Failed to find optimal batch size: {}", e)),
        };
        
        let vanity_mode = if job.request.is_suffix {
            VanityMode::Suffix
        } else {
            VanityMode::Prefix
        };
        
        let start_time = Instant::now();
        
        // Run the vanity address search
        let search_result = vanity_generator::generate_vanity_address_with_updates(
            device,
            &job.request.pattern,
            vanity_mode,
            job.request.case_sensitive,
            batch_size,
            job.request.max_attempts,
            self.stop_flag.clone(),
            |attempts| {
                let _ = self.update_job_attempts(&job_id, attempts);
            }
        );
        
        match search_result {
            Ok(result) => {
                let keypair_bytes = result.keypair.to_bytes().to_vec();
                let duration = result.duration.as_millis() as u64;
                let rate = if duration > 0 { result.attempts * 1000 / duration } else { 0 };
                
                let response = VanityResponse {
                    address: result.address,
                    keypair_bytes,
                    attempts: result.attempts,
                    duration_ms: duration,
                    rate_per_second: rate,
                };
                
                // Update job with result
                self.set_job_result(&job_id, response.clone())?;
                
                Ok(response)
            },
            Err(e) => {
                self.set_job_failed(&job_id, &e.to_string())?;
                Err(e.to_string())
            },
        }
    }
}

// API routes
#[get("/health")]
async fn health_check() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

#[get("/jobs")]
async fn list_jobs(job_manager: web::Data<JobManager>) -> impl Responder {
    let jobs = job_manager.list_jobs();
    HttpResponse::Ok().json(jobs)
}

#[get("/jobs/{job_id}")]
async fn get_job(job_manager: web::Data<JobManager>, path: web::Path<String>) -> impl Responder {
    let job_id = path.into_inner();
    
    match job_manager.get_job(&job_id) {
        Some(job) => HttpResponse::Ok().json(job),
        None => HttpResponse::NotFound().json(serde_json::json!({
            "error": format!("Job not found: {}", job_id)
        })),
    }
}

#[post("/jobs")]
async fn create_job(
    job_manager: web::Data<JobManager>,
    request: web::Json<VanityRequest>,
    job_queue: web::Data<mpsc::Sender<String>>,
) -> impl Responder {
    // Validate pattern
    if request.pattern.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Pattern cannot be empty"
        }));
    }
    
    // Add job to queue
    match job_manager.add_job(request.into_inner()) {
        Ok(job_id) => {
            // Send job to worker queue
            if let Err(e) = job_queue.send(job_id.clone()).await {
                error!("Failed to queue job: {}", e);
                return HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": format!("Failed to queue job: {}", e)
                }));
            }
            
            HttpResponse::Accepted().json(serde_json::json!({
                "job_id": job_id
            }))
        },
        Err(e) => {
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to create job: {}", e)
            }))
        },
    }
}

#[post("/jobs/{job_id}/cancel")]
async fn cancel_job(job_manager: web::Data<JobManager>, path: web::Path<String>) -> impl Responder {
    let job_id = path.into_inner();
    
    match job_manager.cancel_job(&job_id) {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({
            "status": "cancelled",
            "job_id": job_id
        })),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({
            "error": e
        })),
    }
}

// Configure and run API server
pub async fn run_api_server(
    host: &str,
    port: u16,
    allowed_origins: Vec<String>,
) -> std::io::Result<()> {
    // Set up logger
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    
    // Create job manager
    let job_manager = web::Data::new(JobManager::new());
    
    // Initialize CUDA device
    if let Err(e) = job_manager.init_cuda() {
        error!("Failed to initialize CUDA device: {}", e);
        return Err(std::io::Error::new(std::io::ErrorKind::Other, e));
    }
    
    // Create job queue
    let (job_sender, mut job_receiver) = mpsc::channel::<String>(100);
    let job_queue = web::Data::new(job_sender);
    
    // Clone job manager for worker thread
    let job_manager_clone = job_manager.clone();
    
    // Spawn worker thread
    tokio::spawn(async move {
        while let Some(job_id) = job_receiver.recv().await {
            info!("Processing job: {}", job_id);
            
            if let Err(e) = job_manager_clone.worker_loop(job_id.clone()) {
                error!("Job {} failed: {}", job_id, e);
            }
        }
    });
    
    // Start API server
    info!("Starting API server at {}:{}", host, port);
    
    HttpServer::new(move || {
        // Configure CORS
        let mut cors = Cors::default()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);
        
        // Add allowed origins
        for origin in &allowed_origins {
            cors = cors.allowed_origin(origin);
        }
        
        App::new()
            .wrap(middleware::Logger::default())
            .wrap(cors)
            .app_data(job_manager.clone())
            .app_data(job_queue.clone())
            .service(health_check)
            .service(list_jobs)
            .service(get_job)
            .service(create_job)
            .service(cancel_job)
    })
    .bind((host, port))?
    .run()
    .await
}

// Extension for the vanity generator to report progress
pub fn generate_vanity_address_with_updates(
    device: &CudaDevice,
    pattern: &str,
    mode: VanityMode,
    case_sensitive: bool,
    batch_size: usize,
    max_attempts: Option<u64>,
    stop_flag: Arc<AtomicBool>,
    update_callback: impl Fn(u64) + Send + 'static,
) -> Result<vanity_generator::VanityResult, vanity_generator::CudaError> {
    // Implement a modified version of the generate_vanity_address function 
    // that reports progress and checks for the stop flag
    todo!("Implement the vanity address generator with progress updates")
}