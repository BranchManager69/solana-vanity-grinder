use colored::*;

pub struct TimeEstimate {
    pub difficulty: f64,
    pub adjusted_difficulty: f64,
    pub expected_attempts: f64,
    pub expected_seconds: f64,
    pub expected_time: String,
}

/// Estimates the time required to find a vanity address with the given parameters
/// 
/// # Arguments
/// 
/// * `pattern_length` - Length of the pattern to search for
/// * `case_sensitive` - Whether the search is case sensitive
/// * `gpu_rate` - Number of addresses the GPU can check per second
/// 
/// # Returns
/// 
/// A `TimeEstimate` struct containing difficulty and time information
pub fn estimate_time(pattern_length: u32, case_sensitive: bool, gpu_rate: f64) -> TimeEstimate {
    let base_difficulty = (1.0_f64 / 58.0_f64).powf(pattern_length as f64);
    let case_multiplier = if case_sensitive { 
        1.0_f64 
    } else { 
        (33.0_f64 / 58.0_f64).powf(pattern_length as f64) 
    };
    let adjusted_difficulty = base_difficulty * case_multiplier;
    let expected_attempts = 1.0 / adjusted_difficulty;
    let expected_seconds = expected_attempts / gpu_rate;
    
    TimeEstimate {
        difficulty: base_difficulty,
        adjusted_difficulty,
        expected_attempts,
        expected_seconds,
        expected_time: format_time(expected_seconds),
    }
}

/// Formats a time in seconds to a human-readable string
/// 
/// # Arguments
/// 
/// * `seconds` - Time in seconds
/// 
/// # Returns
/// 
/// A formatted string representing the time in the most appropriate unit
fn format_time(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("{:.2} seconds", seconds)
    } else if seconds < 3600.0 {
        format!("{:.2} minutes", seconds / 60.0)
    } else if seconds < 86400.0 {
        format!("{:.2} hours", seconds / 3600.0)
    } else if seconds < 31536000.0 {
        format!("{:.2} days", seconds / 86400.0)
    } else {
        format!("{:.2} years", seconds / 31536000.0)
    }
}

/// Formats a large number with commas for easier reading
fn format_large_number(num: f64) -> String {
    let num_int = num as u64;
    if num_int > 0 {
        // Format integer part with commas
        let mut s = String::new();
        let digits = num_int.to_string();
        let len = digits.len();
        
        for (i, c) in digits.chars().enumerate() {
            s.push(c);
            if (len - i - 1) % 3 == 0 && i < len - 1 {
                s.push(',');
            }
        }
        s
    } else {
        // For very small numbers, use scientific notation
        format!("{:.2e}", num)
    }
}

/// Provides a human-readable representation of the estimation results
/// 
/// # Arguments
/// 
/// * `estimate` - The time estimate to format
/// * `case_sensitive` - Whether the search is case sensitive
/// 
/// # Returns
/// 
/// A formatted string describing the estimate
pub fn format_estimate(estimate: &TimeEstimate, case_sensitive: &bool) -> String {
    let inverse_base = format_large_number(1.0 / estimate.difficulty);
    let inverse_adjusted = format_large_number(1.0 / estimate.adjusted_difficulty);
    let attempts = format_large_number(estimate.expected_attempts);
    
    // Write descriptive explanation
    let description = if estimate.expected_seconds < 10.0 {
        "This is extremely fast - almost instant!".bright_green().bold().to_string()
    } else if estimate.expected_seconds < 60.0 {
        "This is very quick - just seconds!".green().to_string()
    } else if estimate.expected_seconds < 3600.0 {
        format!("This will take about {} minutes on your GPU.", 
                ((estimate.expected_seconds / 60.0) as u32).to_string().yellow())
    } else if estimate.expected_seconds < 86400.0 {
        format!("This will take about {} hours on your GPU.", 
                ((estimate.expected_seconds / 3600.0) as u32).to_string().yellow())
    } else if estimate.expected_seconds < 604800.0 {
        format!("This will take about {} days on your GPU.", 
                ((estimate.expected_seconds / 86400.0) as u32).to_string().yellow())
    } else if estimate.expected_seconds < 2592000.0 {
        format!("This will take about {} weeks on your GPU.", 
                ((estimate.expected_seconds / 604800.0) as u32).to_string().yellow())
    } else if estimate.expected_seconds < 31536000.0 {
        format!("This will take about {} months on your GPU.", 
                ((estimate.expected_seconds / 2592000.0) as u32).to_string().yellow())
    } else {
        format!("This will take about {} years on your GPU - consider a shorter pattern.", 
                ((estimate.expected_seconds / 31536000.0) as u32).to_string().red().bold())
    };
    
    let case_info = if *case_sensitive {
        "Case-sensitive search".to_string()
    } else {
        "Case-insensitive search (faster)".green().to_string()
    };
    
    format!(
        "{}: 1 in {}\n\
         {}: 1 in {}\n\
         {}: {}\n\
         {}: {}\n\
         {}\n\n\
         {}", 
        "Base difficulty".blue().bold(), inverse_base.cyan(),
        "Adjusted difficulty".blue().bold(), inverse_adjusted.cyan(),
        "Expected attempts".blue().bold(), attempts.cyan(),
        "Expected time".blue().bold(), estimate.expected_time.cyan().bold(),
        case_info,
        description
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_case_sensitive() {
        let estimate = estimate_time(4, true, 1_000_000.0);
        assert!((estimate.difficulty - (1.0 / 58.0).powf(4.0)).abs() < 0.0001);
        assert!((estimate.adjusted_difficulty - (1.0 / 58.0).powf(4.0)).abs() < 0.0001);
    }

    #[test]
    fn test_estimate_case_insensitive() {
        let estimate = estimate_time(4, false, 1_000_000.0);
        assert!((estimate.difficulty - (1.0 / 58.0).powf(4.0)).abs() < 0.0001);
        assert!((estimate.adjusted_difficulty - (1.0 / 58.0).powf(4.0) * (33.0 / 58.0).powf(4.0)).abs() < 0.0001);
    }

    #[test]
    fn test_format_time() {
        assert_eq!(format_time(30.0), "30.00 seconds");
        assert_eq!(format_time(90.0), "1.50 minutes");
        assert_eq!(format_time(7200.0), "2.00 hours");
        assert_eq!(format_time(172800.0), "2.00 days");
        assert_eq!(format_time(63072000.0), "2.00 years");
    }
    
    #[test]
    fn test_format_large_number() {
        assert_eq!(format_large_number(1000.0), "1,000");
        assert_eq!(format_large_number(1000000.0), "1,000,000");
        assert_eq!(format_large_number(1234567890.0), "1,234,567,890");
    }
}