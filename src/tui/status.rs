use rand::{Rng, RngExt, rng};

pub fn thinking_status() -> &'static str {
    const STATUSES: &[&str] = &[
        "Pondering",
        "Connecting the dots",
        "Charting a course",
        "Working the problem",
        "Figuring it out",
        "Cooking",
        "Tuning the solution",
        "Exploring the workspace",
        "Mapping the codebase",
        "Tracing the path",
        "Thinking sideways",
        "Piecing things together",
        "Plotting the next move",
    ];

    let mut rng = rng();

    STATUSES[rng.random_range(0..STATUSES.len())]
}

pub fn finished_status() -> &'static str {
    const STATUSES: &[&str] = &[
        "Done",
        "All set",
        "Mission complete",
        "Wrapped up",
        "Ready",
        "Ship it",
    ];

    let mut rng = rng();

    STATUSES[rng.random_range(0..STATUSES.len())]
}

pub fn error_status() -> &'static str {
    const STATUSES: &[&str] = &[
        "Hit a snag",
        "Something went sideways",
        "Ran into a wall",
        "That didn't work",
        "Lost the trail",
        "Hmm...",
    ];

    let mut rng = rng();

    STATUSES[rng.random_range(0..STATUSES.len())]
}

pub fn tool_status(tool: &str) -> &'static str {
    match tool {
        "list_directory" => "Exploring",
        "read_file" => "Reading",
        "write_file" => "Writing",
        "search_files" => "Searching",
        "run_command" => "Running",
        _ => "Working",
    }
}
