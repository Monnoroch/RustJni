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
		&[/*opt1, JavaVMOption::new("-verbose:jni")*/][..],
		false,
	);
	println!("Args are {:?}", args);

	let jvm = JavaVM::new(args).unwrap();
	println!("Jvm is {:?}", jvm);

	let (env, cap) = jvm.get_env().unwrap();
	println!("Env is {:?}", env);
	println!("Env version is {:?}", env.version(&cap));

	let (cls, cap) = match JavaClass::find(&env, "java/lang/String", cap) {
		Ok(a) => a,
		_ => panic!("unexpected exception")
	};

	let proto = "Hello, world!";
	let (st, cap) = match JavaString::new(&env, proto, cap) {
		Ok(a) => a,
		_ => panic!("unexpected exception")
	};

	println!("St is {:?}", st.to_str(&cap).unwrap());
	// assert_eq!(st.to_str(&cap), proto);

	println!("St len is {:?} == {:?}", st.to_str(&cap).unwrap().len(), proto.len());

	let class = st.get_class(&cap);
	let class2 = st.get_class(&cap);
	println!(
		"Clses are {:?}, {:?}, {:?}, {:?}", cls, class,
		cls == class2,
		st.is_instance_of(&cls, &cap)
	);

	println!("st[2:7] == {:?}", st.region(2, 5, cap));

	let cap = JavaThrowable::check(&env).unwrap();

	let (gst, cap) = try!(st.global(cap));
	let (wgst, cap) = try!(gst.weak(cap));
	let (wst, _cap) = try!(st.weak(cap));
	println!("Wst is null: {:?}", wst.is_null());
	println!("{:?} {:?} {:?} {:?} {:?}", st, gst, wgst, wst, wgst);
	println!("Wst is null: {:?}", wst.is_null());

	Ok(())
}

/*
use ::std::marker::PhantomData;

struct Parent {
	val: u64,
}

impl Parent {
	pub fn new(v: u64) -> Parent {
		Parent { val: v }
	}

	pub fn child(&self, v: u64) -> Child {
		Child {
			val: v,
			phantom: PhantomData,
		}
	}
}

struct Child<'a> {
	val: u64,
	phantom: PhantomData<&'a Parent>,
}

impl<'a> Child<'a> {
	pub fn compare(&'a self, l: &Obj<'a>, r: &Obj<'a>) -> bool {
		l.val == r.val
	}

	pub fn obj(&'a self, v: u64) -> Obj<'a> {
		Obj {
			val: v,
			child: self,
		}
	}
}

struct Obj<'a> {
	val: u64,
	child: &'a Child<'a>,
}

impl<'a> PartialEq<Obj<'a>> for Obj<'a> {
	fn eq(&self, other: &Obj<'a>) -> bool {
		self.child.compare(self, other)
	}
}


#[test]
fn test() {
	let parent = Parent::new(1);
	let child = parent.child(2);
	let obj1 = child.obj(3);
	let obj2 = child.obj(3);
	assert!(obj1 == obj2);
	assert!(obj2 == obj1);

	let parent2 = Parent::new(1);
	let child2 = parent2.child(2);
	let obj12 = child2.obj(3);
	let obj22 = child2.obj(3);
	assert!(obj12 == obj22);
	assert!(obj22 == obj12);

	// assert!(obj1 == obj12);
	assert!(obj12 == obj1);
}

*/

/*
use ::std::marker::PhantomData;

struct Parent {
	val: u64,
}

impl Parent {
	pub fn new(v: u64) -> Parent {
		Parent { val: v }
	}

	pub fn child(&self, v: u64) -> Child {
		Child {
			val: v,
			phantom: PhantomData,
		}
	}
}

struct Child<'a> {
	val: u64,
	phantom: PhantomData<&'a Parent>,
}

impl<'a> Child<'a> {
	pub fn compare<'b, L: 'a + Obj<'a>, R: 'a + Obj<'b>>(&'a self, l: &L, r: &R) -> bool {
		l.get_val() == r.get_val()
	}

	pub fn obj1(&'a self, v: u64) -> Obj1<'a> {
		Obj1 {
			val: v,
			child: self,
		}
	}

	pub fn obj2(&'a self, v: u64) -> Obj2<'a> {
		Obj2 {
			val: v,
			child: self,
		}
	}
}

trait Obj<'a> {
	fn get_child(&'a self) -> &'a Child<'a>;
	fn get_val(&self) -> u64;
}

struct Obj1<'a> {
	val: u64,
	child: &'a Child<'a>,
}

impl<'a, R: 'a + Obj<'a>> PartialEq<R> for Obj1<'a> {
	fn eq(&self, other: &R) -> bool {
		self.get_child().compare(self, other)
	}
}

impl<'a> Obj<'a> for Obj1<'a> {
	fn get_child(&'a self) -> &'a Child<'a> {
		self.child
	}

	fn get_val(&self) -> u64 {
		self.val
	}
}

struct Obj2<'a> {
	val: u64,
	child: &'a Child<'a>,
}

impl<'a, R: 'a + Obj<'a>> PartialEq<R> for Obj2<'a> {
	fn eq(&self, other: &R) -> bool {
		self.get_child().compare(self, other)
	}
}

impl<'a> Obj<'a> for Obj2<'a> {
	fn get_child(&'a self) -> &'a Child<'a> {
		self.child
	}

	fn get_val(&self) -> u64 {
		self.val
	}
}

#[test]
fn test() {
	let parent = Parent::new(1);
	let child = parent.child(2);
	let obj1 = child.obj1(3);
	let obj2 = child.obj2(3);
	assert!(obj1 == obj2);
	assert!(obj2 == obj1);

	let parent2 = Parent::new(1);
	let child2 = parent2.child(2);
	let obj12 = child2.obj1(3);
	let obj22 = child2.obj2(3);
	assert!(obj12 == obj22);
	assert!(obj22 == obj12);

	// assert!(obj1 == obj12);
	assert!(obj12 == obj1);
}

*/
