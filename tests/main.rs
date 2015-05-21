#![allow(dead_code)]
extern crate libc;
extern crate jni;

use jni::*;

#[test]
fn test() {
    assert!(!mytest().is_err());
}

fn mytest() -> Result<(),jni::Exception> {
    let opt1 = JavaVMOption::new("-Xcheck:jni");
    println!("Opt is {:?}", opt1);

    let opt2 = JavaVMOption::new("-verbose:jni");
    println!("Opt is {:?}", opt2);

    let args = JavaVMInitArgs::new(
        jni::JniVersion::JNI_VERSION_1_4,
        &[opt1, JavaVMOption::new("-verbose:jni"),][..],
        false,
    );
    println!("Args are {:?}", args);

    let mut t = JavaVM::new(args, "");
    assert!(!t.is_err());

    let (mut jvm, cap) = t.unwrap();
    println!("Jvm is {:?}", jvm);

    let t = jvm.get_env();
    assert!(!t.is_err());

    let env = t.unwrap();
    println!("Env is {:?}", env);
    println!("Env version is {:?}", env.version(&cap));

    let mut vec = Vec::new();

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
}
