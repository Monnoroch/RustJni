#![allow(dead_code)]

extern crate libc;
extern crate jni;

use jni::*;


fn main() {
	let opt = JavaVMOption::new("-verbose:jni", 0 as *const ::libc::c_void);
	println!("Opt is {:?}", opt);

	let args = JavaVMInitArgs::new(jni::JniVersion::JNI_VERSION_1_4, [opt].as_slice(), false);
	println!("Args is {:?}", args);

	let mut jvm = JavaVM::new(args, "");
	println!("Jvm is {:?}", jvm);

	let env = jvm.get_env();
	println!("Env is {:?}", env);

	println!("Version is {:?}", env.version());

	let cls = JavaClass::find(&env, "java/lang/String");

	let proto = "Hello, world!";
	let st = JavaString::new(env, proto);
	println!("St is {:?}", st);
	assert!(st.to_str() == proto.to_string());

	println!("St len is {:?} == {:?}", st.len(), proto.len());

	println!("Clses are {:?}, {:?}, {:?}, {:?}", cls, st.get_class(), cls.is_same(&st.get_class()), st.is_instance_of(&cls));

	println!("st[2:7] == {:?}", st.region(2, 5));


	let gst = st.global();
	let wgst = gst.weak();
	let wst = st.weak();
	println!("Wst is null: {:?}", wst.is_null());
	println!("{:?} {:?} {:?} {:?} {:?}", st, gst, wgst, wst, wgst);
	println!("Wst is null: {:?}", wst.is_null());

	env.fatal_error("Hello, error!");
}
