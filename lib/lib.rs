#![crate_name = "jni"]
#![crate_type = "rlib"]

#![feature(unsafe_destructor)]
#![allow(non_camel_case_types)]
#![allow(raw_pointer_derive)]
#![allow(non_snake_case)]
#![allow(improper_ctypes)]
#![allow(unstable)]
#![allow(unused_attributes)]

extern crate libc;

pub use self::jni::*;
pub use self::native::{JniVersion/*, JNI_VERSION_1_1, JNI_VERSION_1_2, JNI_VERSION_1_4, JNI_VERSION_1_6*/};

pub mod native;
mod jni;
