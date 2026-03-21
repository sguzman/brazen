use std::path::PathBuf;

fn main() {
    if std::env::var("CARGO_FEATURE_SERVO").is_ok() {
        let source = std::env::var("BRAZEN_SERVO_SOURCE").ok();
        let Some(source) = source else {
            panic!("BRAZEN_SERVO_SOURCE must be set when building with the servo feature");
        };
        let path = PathBuf::from(&source);
        if !path.exists() {
            panic!(
                "BRAZEN_SERVO_SOURCE was set to `{}`, but the path does not exist",
                source
            );
        }
        println!("cargo:rerun-if-env-changed=BRAZEN_SERVO_SOURCE");
    }
}
