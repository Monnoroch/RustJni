#![allow(dead_code)]
extern crate libc;
extern crate jni;

use jni::*;
use std::result::Result;


#[test]
fn test () {
    let _ = self::mytest();
}


fn mytest() -> Result<(),jni::Exception> {
    let opt = JavaVMOption::new("-Xcheck:jni",
                                0 as *const ::libc::c_void);
    println!("Opt is {:?}", opt);

    let opt2 = JavaVMOption::new("-ea",
                                 0 as *const ::libc::c_void);
    println!("Opt is {:?}", opt2);

    let args = JavaVMInitArgs::new(
        jni::JniVersion::JNI_VERSION_1_4, &[opt, opt2][..], false);
    println!("Args is {:?}", args);

    let (mut jvm, cap) = JavaVM::new(args, "").unwrap();
    let mut vec = Vec::new();

    println!("Jvm is {:?}", &jvm);

    let env = jvm.get_env();
    println!("Env is {:?}", &env);
    println!("Version is {:?}", env.version(&cap));
    let string_name = JavaChars::new("java/lang/String");
    let (cls, cap) = match JavaClass::find(&env, &string_name, cap) { Ok(a) => a, _ => panic!("unexpected exception") };

    let proto = JavaChars::new("Hello, world!");
    let (st, cap) = match JavaString::new(&env, &proto, cap){ Ok(a) => a, _ => panic!("unexpected exception") };
    println!("St is {:?}", st.to_str().unwrap());
    assert_eq!(st.to_str(), proto.to_string());

    println!("St len is {:?} == {:?}", st.to_str().unwrap().len(),
             proto.to_string().unwrap().len());
    let (class, cap) = try!(st.get_class(cap));
    let (class2, cap) = try!(st.get_class(cap));
    println!("Clses are {:?}, {:?}, {:?}, {:?}", cls,
             class,
             cls.is_same(&class2),
             st.is_instance_of(&cls, &cap));

    println!("st[2:7] == {:?}", st.region(2, 5));


    let (gst, cap) = try!(st.global(cap));
    let (wgst, cap) = try!(gst.weak(cap));
    let (wst, _cap) = try!(st.weak(cap));
    println!("Wst is null: {:?}", wst.is_null());
    println!("{:?} {:?} {:?} {:?} {:?}", st, gst, wgst, wst,
             wgst);
    println!("Wst is null: {:?}", wst.is_null());
    Ok(vec.push(env.clone()))

        // let java_chars = JavaChars::new("Hello, error!");
        // vec[0].fatal_error(&java_chars)
}
