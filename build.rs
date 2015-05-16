use std::env;
fn main () -> () {
    match env::var("LD_LIBRARY_PATH") {
        Ok(var) => for i in env::split_paths(&var) {
            println!("cargo:rustc-link-search={}",i.to_str().unwrap())
        },
        Err(_) => ()
    }
}
