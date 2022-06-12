pub struct Process {
    command: String,
}

impl Process {
    pub fn new(command: String) -> Process {
        Process { command }
    }
}
