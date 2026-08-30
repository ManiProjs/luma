use rand::{RngExt, rng};

const THINKING_STATUSES: &[&str] = &[
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

const FINISHED_STATUSES: &[&str] = &[
    "Done",
    "All set",
    "Mission complete",
    "Wrapped up",
    "Ready",
    "Ship it",
];

const ERROR_STATUSES: &[&str] = &[
    "Hit a snag",
    "Something went sideways",
    "Ran into a wall",
    "That didn't work",
    "Lost the trail",
    "Hmm...",
];

pub fn thinking_status() -> &'static str {
    random_status(THINKING_STATUSES)
}

pub fn finished_status() -> &'static str {
    random_status(FINISHED_STATUSES)
}

pub fn error_status() -> &'static str {
    random_status(ERROR_STATUSES)
}

pub fn tool_status(tool: &str) -> &'static str {
    match tool {
        "list_directory" => "Exploring",
        "read_file" => "Reading",
        "write_file" => "Writing",
        "patch_file" => "Patching",
        "search_files" => "Searching",
        "run_command" => "Running",
        _ => "Working",
    }
}

fn random_status(statuses: &'static [&'static str]) -> &'static str {
    let mut rng = rng();
    statuses[rng.random_range(0..statuses.len())]
}
