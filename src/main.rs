mod log;

fn main() {
    let cmd = log::Command::Put("x".to_string(), "10".to_string());
    println!("Command: {:?}", cmd);
}
