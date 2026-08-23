//! DevScope command-line entry point.
//!
//! The presentation is intentionally isolated here so a future Ratatui frontend can
//! be introduced without coupling it to the progress-analysis core.

fn main() {
    print_welcome();
}

fn print_welcome() {
    println!("DevScope");
    println!("AI-assisted development progress observer");
    println!();
    println!("DevScope is not implemented yet.");
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {
        // Keeps the binary crate's initial test command meaningful.
    }
}
