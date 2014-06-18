#![crate_id = "jni#0.1"]
#![crate_type = "rlib"]

#![feature(globs)]
#![feature(macro_rules)]
#![allow(non_camel_case_types)]
#![allow(raw_pointer_deriving)]
#![allow(uppercase_variables)]
#![allow(non_snake_case_functions)]
#![allow(ctypes)]

extern crate libc;
extern crate debug;

pub use self::jni::*;
pub use self::native::{JniVersion, JNI_VERSION_1_1, JNI_VERSION_1_2, JNI_VERSION_1_4, JNI_VERSION_1_6};

pub mod native;
mod jni;
