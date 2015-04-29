#![crate_name = "jni"]
#![crate_type = "rlib"]

#![feature(unsafe_destructor)]
#![feature(libc)]
#![feature(std_misc)]
#![feature(collections)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused_attributes)]

extern crate libc;

pub use self::jni::*;
pub use self::j_chars::*;
pub use self::native::{JniVersion/*, JNI_VERSION_1_1, JNI_VERSION_1_2, JNI_VERSION_1_4, JNI_VERSION_1_6*/};

pub mod native;
mod jni;
mod j_chars;
