use std::env;
fn main () -> () {
    if let Ok(var) = env::var("LD_LIBRARY_PATH") {
        for i in env::split_paths(&var) {
            println!("cargo:rustc-link-search={}",i.to_str().unwrap());
        }
    }
}
