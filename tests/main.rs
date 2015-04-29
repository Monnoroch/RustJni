#![allow(dead_code)]
#![feature(libc)]
extern crate libc;
extern crate jni;

use jni::*;


fn main() {
	let opt = JavaVMOption::new("-Xcheck:jni",
								0 as *const ::libc::c_void);
	println!("Opt is {:?}", opt);

	let args = JavaVMInitArgs::new(
		jni::JniVersion::JNI_VERSION_1_4, &[opt][..], false);
	println!("Args is {:?}", args);

	let mut jvm = JavaVM::new(args, "");
	let mut vec = Vec::new();

	println!("Jvm is {:?}", &jvm);

	let env = jvm.get_env();
	println!("Env is {:?}", &env);

	println!("Version is {:?}", env.version());
	let string_name = JavaChars::new("java/lang/String");
	let cls = JavaClass::find(&env, &string_name);

	let proto = JavaChars::new("Hello, world!");
	let st = JavaString::new(&env, &proto);
	println!("St is {:?}", st.to_str());
	assert_eq!(st.to_str(), proto.to_string().unwrap());

	println!("St len is {:?} == {:?}", st.to_str().len(),
			 proto.to_string().unwrap().len());

	println!("Clses are {:?}, {:?}, {:?}, {:?}", cls,
			 st.get_class(),
			 cls.is_same(&st.get_class()),
			 st.is_instance_of(&cls));

	println!("st[2:7] == {:?}", st.region(2, 5));


	let gst = st.global();
	let wgst = gst.weak();
	let wst = st.weak();
	println!("Wst is null: {:?}", wst.is_null());
	println!("{:?} {:?} {:?} {:?} {:?}", st, gst, wgst, wst,
			 wgst);
	println!("Wst is null: {:?}", wst.is_null());
	vec.push(env.clone());

	let java_chars = JavaChars::new("Hello, error!");
	vec[0].fatal_error(&java_chars)
}
// vim: set noexpandtab:
// vim: set tabstop=4:
// vim: set shiftwidth=4:
// Local Variables:
// mode: rust
// indent-tabs-mode: t
// rust-indent-offset: 4
// tab-width: 4
// End:
