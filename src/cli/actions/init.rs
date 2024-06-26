use std::fs;

pub fn init_configuration() {
    // create `hooker.config.yaml`
    fs::write("hooker.config.yaml", "hooks:\n\t- cmd: echo \"Hello, Hooker!\"");
}
