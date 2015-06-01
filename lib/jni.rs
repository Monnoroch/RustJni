//! # Some notes on design and implementation:
//!
//! ## Handling of Java types
//!
//! * Java's primitive types, `String`, `Class`, and `Object` are
//!   distinguished in this interface.  No distinction is made by this
//!   library between other different Java classes.
//!
//! * Method calls are dynamically checked: calls with wrong number of
//!   arguments raise `IndexOutOfBoundsException`, and calls with
//!   wrong types raise `ClassCastException`.
//!
//! ## Exception handling
//!
//! * The type `Capability` is a token that implies that their _is no_
//!   pending exception, and so it is safe to call JNI functions.  The
//!   type `Exception` is a token that implies that their _is_ a
//!   pending exception, and so it is valid to find the pending
//!   exception,
//!
//! * While Rust treats OOM as fatal, `OutOfMemoryError` does _not_
//!   imply _native_ memory is exhausted.  Rather, it implies
//!   exhaustion of the _Java_ heap and/or PermGen, which is
//!   _disjoint_ from the native memory used by Rust.  Therefore, Java
//!   OOM is a recoverable condition at the Rust level.
//!
//! ## Null handling
//!
//! Some JNI methods do not  allow `null` to  be passed to  them.  To
//! solve this, this interface  converts `null`  to `None`  and other
//! values to `Some(x)`.
//!
//! ## Error handling
//!
//! Rust code generally uses `panic!` in the event of a programmer
//! error. Inside of a native Java method, however, this will lead to
//! undefined behavior due to unwinding outside of Rust code.  The
//! solution is to throw a Java `RuntimeException` instead, as is the
//! Java practice.  Note that this does lose Rust-level backtraces.

use ::std::mem;
use ::std::fmt;
use ::std::string;
use std::ffi::{CString, CStr};
use ::std::marker::PhantomData;

use super::native::*;
use super::j_chars::JavaChars;

/// A token that indicates that the VM does not have a pending
/// exception.
///
/// One must present it to any method that is not safe to call in the
/// presense of an exception.
///
/// * If the method can raise an exception,
///   the function will take ownership of the passed-in value.  It will
///   return:
///
///   * `Ok(ReturnType, Capability)` (where `ReturnType` is the actual
///     useful return value) on success.
///   * `Err(Exception)` on error; see below for the `Exception` type.
///
/// * If the method cannot raise an exception, but cannot be called if
///   one is pending, a `Capability` will be taken by const reference.
#[derive(Debug)]
pub struct Capability {
	_cap: ()
}

impl Capability {
	fn new() -> Capability {
		Capability { _cap: () }
	}
}

/// A token that indicates that their is an exception pending in the
/// current thread.
///
/// This token can be converted back into a `Capability` object by
/// clearing the exception.
#[derive(Debug)]
pub struct Exception {
	_cap: ()
}

impl Exception {
	fn new() -> Exception {
		Exception { _cap: () }
	}
}

pub type JniResult<T> = Result<(T, Capability), Exception>;


trait JPrimitive {
	type Type;
	type ArrType;

	fn repr(&self) -> Self::Type;
	fn from(val: Self::Type) -> Self;
}

impl JPrimitive for jboolean {
	type Type = bool;
	type ArrType = jbooleanArray;

	fn repr(&self) -> Self::Type {
		*self == JNI_TRUE
	}

	fn from(val: Self::Type) -> Self {
		if val {
			JNI_TRUE
		} else {
			JNI_FALSE
		}
	}
}

impl JPrimitive for jbyte {
	type Type = u8;
	type ArrType = jbyteArray;

	fn repr(&self) -> Self::Type {
		*self as Self::Type
	}

	fn from(val: Self::Type) -> Self {
		val as Self
	}
}

impl JPrimitive for jchar {
	type Type = char;
	type ArrType = jcharArray;

	fn repr(&self) -> Self::Type {
		::std::char::from_u32(*self as u32).unwrap() // NOTE: should always be ok
	}

	fn from(val: Self::Type) -> Self {
		val as Self // NOTE: should always be ok
	}
}

impl JPrimitive for jshort {
	type Type = i16;
	type ArrType = jshortArray;

	fn repr(&self) -> Self::Type {
		*self as Self::Type
	}

	fn from(val: Self::Type) -> Self {
		val as Self
	}
}

impl JPrimitive for jint {
	type Type = i32;
	type ArrType = jintArray;

	fn repr(&self) -> Self::Type {
		*self as Self::Type
	}

	fn from(val: Self::Type) -> Self {
		val as Self
	}
}

impl JPrimitive for jlong {
	type Type = i64;
	type ArrType = jlongArray;

	fn repr(&self) -> Self::Type {
		*self as Self::Type
	}

	fn from(val: Self::Type) -> Self {
		val as Self
	}
}

impl JPrimitive for jfloat {
	type Type = f32;
	type ArrType = jfloatArray;

	fn repr(&self) -> Self::Type {
		*self as Self::Type
	}

	fn from(val: Self::Type) -> Self {
		val as Self
	}
}

impl JPrimitive for jdouble {
	type Type = f64;
	type ArrType = jdoubleArray;

	fn repr(&self) -> Self::Type {
		*self as Self::Type
	}

	fn from(val: Self::Type) -> Self {
		val as Self
	}
}

trait RPrimitive {
	type JType: JPrimitive;
}

impl RPrimitive for bool {
	type JType = jboolean;
}

impl RPrimitive for u8 {
	type JType = jbyte;
}

impl RPrimitive for char {
	type JType = jchar;
}

impl RPrimitive for i16 {
	type JType = jshort;
}

impl RPrimitive for i32 {
	type JType = jint;
}

impl RPrimitive for i64 {
	type JType = jlong;
}

impl RPrimitive for f32 {
	type JType = jfloat;
}

impl RPrimitive for f64 {
	type JType = jdouble;
}

/// Stores an option for the JVM
#[allow(raw_pointer_derive)]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct JavaVMOption {
	/// The option to be passed to the JVM
	pub optionString: string::String,

	/// Extra info for the JVM.
	pub extraInfo: *const ::libc::c_void
}

impl JavaVMOption {
	/// Constructs a new `JavaVMOption`
	pub fn new(option: &str) -> JavaVMOption {
		return Self::new_extra(option, 0 as *const ::libc::c_void)
	}

	/// Constructs a new `JavaVMOption` with extra info.
	pub fn new_extra(option: &str, extra: *const ::libc::c_void) -> JavaVMOption {
		JavaVMOption{
			optionString: option.to_string(),
			extraInfo: extra,
		}
	}

	fn from(val: &JavaVMOptionImpl) -> Option<JavaVMOption> {
		match unsafe { JavaChars::from_raw_vec(CStr::from_ptr(val.optionString).to_bytes_with_nul().to_vec()).to_string() } {
			None => None,
			Some(v) => Some(JavaVMOption{
				optionString: v,
				extraInfo: val.extraInfo,
			}),
		}
	}
}

impl<'a> PartialEq<&'a str> for JavaVMOption {
	fn eq(&self, other: &&'a str) -> bool {
		*other == self.optionString
	}
}

impl<'a> PartialEq<JavaVMOption> for &'a str {
	fn eq(&self, other: &JavaVMOption) -> bool {
		other.optionString == *self
	}
}

/// Stores a vector of options to be passed to the JVM at JVM startup
#[allow(raw_pointer_derive)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaVMInitArgs {
	/// The JVM version required
	pub version: JniVersion,

	/// The options to be passed to the JVM.
	pub options: Vec<JavaVMOption>,

	/// If `true`, the JVM will ignore unrecognized options.
	/// If `false`, the JVM will fail to start if it does not recognize an option
	pub ignoreUnrecognized: bool
}


impl JavaVMInitArgs {
	/// Constructs a new `JavaVMInitArgs`
	pub fn new(version: JniVersion, options: &[JavaVMOption], ignoreUnrecognized: bool) -> JavaVMInitArgs {
		JavaVMInitArgs {
			version: version,
			options: options.to_vec(),
			ignoreUnrecognized: ignoreUnrecognized
		}
	}

	pub fn default(version: JniVersion) -> Result<JavaVMInitArgs, JniError> {
		let mut argsImpl = JavaVMInitArgsImpl{
			version: version,
			nOptions: 0,
			options: 0 as *mut JavaVMOptionImpl,
			ignoreUnrecognized: JNI_FALSE as jboolean,
		};
		let res = unsafe { JNI_GetDefaultJavaVMInitArgs(&mut argsImpl) };
		if res != JniError::JNI_OK {
			return Err(res);
		}

		match Self::from(&argsImpl) {
			None => Err(JniError::JNI_OK),
			Some(v) => Ok(v),
		}
	}

	fn from(val: &JavaVMInitArgsImpl) -> Option<JavaVMInitArgs> {
		let mut res = JavaVMInitArgs{
			version: val.version,
			ignoreUnrecognized: val.ignoreUnrecognized == JNI_TRUE,
			options: Vec::with_capacity(val.nOptions as usize),
		};
		for i in 0..val.nOptions {
			let opt = JavaVMOption::from(unsafe { &*val.options.offset(i as isize) });
			if opt.is_none() {
				return None;
			}
			res.options.push(opt.unwrap());
		}
		Some(res)
	}
}

/// Stores a group of arguments for attaching to the JVM
#[derive(Debug)]
pub struct JavaVMAttachArgs<'a> {
	pub version: JniVersion,
	pub name: String,
	pub group: JavaObject<'a>,
}

impl<'a> JavaVMAttachArgs<'a> {
	pub fn new(version: JniVersion, name: &str, group: JavaObject<'a>) -> JavaVMAttachArgs<'a> {
		JavaVMAttachArgs{
			version: version,
			name: name.to_string(),
			group: group
		}
	}
}

/// Represents a running JVM
/// It is *not* permissible to use an `Env`
/// to be used after the `JavaVM` instance corresponding to it
/// has been destroyed. This is checked by the compiler.
#[allow(raw_pointer_derive)]
#[derive(Debug)]
pub struct JavaVM {
	ptr: *mut JavaVMImpl,
	version: JniVersion,
	owned: bool,
}

impl JavaVM {
	/// Creates a Java Virtual Machine.
	/// The JVM will automatically be destroyed when the object goes out of scope.
	pub fn new(args: JavaVMInitArgs) -> Result<JavaVM, JniError> {
		let (res, jvm) = unsafe {
			let mut jvm: *mut JavaVMImpl = 0 as *mut JavaVMImpl;
			let mut env: *mut JNIEnvImpl = 0 as *mut JNIEnvImpl;
			let mut vm_opts = vec![];
			let mut vm_opts_vect = vec![];
			for opt in args.options.iter() {
				let cstr: CString = CString::new(&opt.optionString[..]).unwrap();
				vm_opts.push(
					JavaVMOptionImpl{
						optionString: cstr.as_ptr(),
						extraInfo: opt.extraInfo,
					}
				);
				vm_opts_vect.push(cstr);
			}

			let mut argsImpl = JavaVMInitArgsImpl{
				version: args.version,
				nOptions: args.options.len() as jint,
				options: vm_opts.as_mut_ptr(),
				ignoreUnrecognized: if args.ignoreUnrecognized { JNI_TRUE } else { JNI_FALSE },
			};

			let res = JNI_CreateJavaVM(&mut jvm, &mut env, &mut argsImpl);

			(res, jvm)
		};

		match res {
			JniError::JNI_OK => {
				let r = JavaVM{
					ptr: jvm,
					version: args.version,
					owned: true,
				};
				Ok(r)
			}
			_ => Err(res)
		}
	}

	pub fn from(ptr: *mut JavaVMImpl) -> JavaVM {
		let mut res = JavaVM {
			ptr: ptr,
			version: JniVersion::JNI_VERSION_1_1,
			owned: false,
		};

		let version = match res.get_env() {
			Err(_) => JniVersion::JNI_VERSION_1_1, // well, what do you do...
			Ok((env, cap)) => env.version(&cap),
		};

		res.version = version;
		res
	}

	pub fn created() -> Result<Vec<JavaVM>, JniError> {
		let mut count: jsize = 0;
		let res = unsafe { JNI_GetCreatedJavaVMs(0 as *mut *mut JavaVMImpl, 0, &mut count) };
		if res != JniError::JNI_OK {
			return Err(res);
		}

		let mut count1: jsize = 0;
		let mut data = Vec::with_capacity(count as usize);
		let res = unsafe { JNI_GetCreatedJavaVMs(data.as_mut_ptr(), count, &mut count1) };
		if res != JniError::JNI_OK {
			return Err(res);
		}

		// shit hapens...
		assert!(count == count1);

		unsafe {data.set_len(count as usize) };
		let mut res = Vec::with_capacity(count as usize);
		for v in &data[..] {
			res.push(JavaVM::from(*v));
		}
		Ok(res)
	}

	pub unsafe fn ptr(&self) -> *mut JavaVMImpl {
		self.ptr
	}

	pub fn version(&self) -> JniVersion {
		return self.version
	}

	pub fn get_env(&self) -> Result<(JavaEnv, Capability), JniError> {
		unsafe {
			let ref jni = **self.ptr;
			self.get_env_gen(jni.AttachCurrentThread)
		}
	}

	pub fn get_env_daemon(&self) -> Result<(JavaEnv, Capability), JniError> {
		unsafe {
			let ref jni = **self.ptr;
			self.get_env_gen(jni.AttachCurrentThreadAsDaemon)
		}
	}

	unsafe fn get_env_gen(&self, fun: extern "C" fn(vm: *mut JavaVMImpl, penv: &mut *mut JNIEnvImpl, args: *mut JavaVMAttachArgsImpl) -> JniError) -> Result<(JavaEnv, Capability), JniError> {
		let mut env: *mut JNIEnvImpl = 0 as *mut JNIEnvImpl;
		let res = ((**self.ptr).GetEnv)(self.ptr, &mut env, self.version());
		match res {
			JniError::JNI_OK => Ok((JavaEnv{
				ptr: &mut *env,
				jvm: self,
				detach: false,
			}, Capability::new())),
			JniError::JNI_EDETACHED => {
				let mut attachArgs = JavaVMAttachArgsImpl{
					version: self.version(),
					name: 0 as *const ::libc::c_char,
					group: 0 as jobject
				};
				let res = fun(self.ptr, &mut env, &mut attachArgs);
				match res {
					JniError::JNI_OK => Ok((JavaEnv{
						ptr: &mut *env,
						jvm: self,
						detach: true,
					}, Capability::new())),
					_ => Err(res)
				}
			},
			_ => Err(res)
		}
	}

	unsafe fn destroy_java_vm(&mut self) -> JniError {
		if self.ptr == 0 as *mut JavaVMImpl {
			return JniError::JNI_OK;
		}

		let err = ((**self.ptr).DestroyJavaVM)(self.ptr);
		self.ptr = 0 as *mut JavaVMImpl;
		err
	}
}

unsafe impl Sync for JavaVM {}

impl PartialEq for JavaVM {
	fn eq(&self, r: &Self) -> bool {
		self.ptr == r.ptr
	}
}

impl Eq for JavaVM {}

impl Drop for JavaVM {
	fn drop(&mut self) {
		if !self.owned {
			return;
		}

		let err = unsafe { self.destroy_java_vm() };
		if err != JniError::JNI_OK {
			panic!("DestroyJavaVM error: {:?}", err);
		}
	}
}

/// Represents an environment pointer used by the JNI.
/// Serves as an upper bound to the lifetime of all local refs
/// created by this binding.
///
/// TODO: allow for global/weak refs to outlive their env.
#[derive(Debug)]
#[allow(raw_pointer_derive)]
pub struct JavaEnv<'a> {
	ptr: *mut JNIEnvImpl,
	jvm: &'a JavaVM,
	detach: bool,
}

impl<'a> JavaEnv<'a> {
	/// Get the underlying JavaVM reference.
	pub fn jvm(&self) -> &'a JavaVM {
		self.jvm
	}

	/// Gets the version of the JVM (mightt be bigger, than the JavaVM args version, but not less)
	pub fn version(&self, _cap: &Capability) -> JniVersion {
		let ver = unsafe { ((**self.ptr).GetVersion)(self.ptr) } as u32;
		match ver {
			MIN_JNI_VERSION ... MAX_JNI_VERSION => unsafe { mem::transmute(ver) },
			_ => panic!("Unsupported version {:?}!", ver),
		}
	}

	/// Defines a Java class from a name, ClassLoader, buffer, and length
	fn define_class<T: 'a + JObject<'a>>(&self, name: &str, loader: &T, buf: &[u8], cap: Capability) -> JniResult<JavaClass> {
		let jname = JavaChars::new(name);
		let (obj, _) = unsafe {
			(((**self.ptr).DefineClass)(self.ptr, jname.as_ptr(), loader.get_obj(), buf.as_ptr() as *const jbyte, buf.len() as jsize), cap)
		};
		// here `cap` is taken, we can't call any Jni methods
		if obj == 0 as jclass {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, obj) }, Capability::new()))
		}
	}

	/// Takes a string and returns a Java class if successfull.
	/// Returns `Err` on failure.
	fn find_class(&self, name: &str, cap: Capability) -> JniResult<JavaClass> {
		let jname = JavaChars::new(name);
		let (obj, _) = unsafe {
			(((**self.ptr).FindClass)(self.ptr, jname.as_ptr()), cap)
		};
		// here `cap` is taken, we can't call any Jni methods
		if obj == 0 as jclass {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, obj) }, Capability::new()))
		}
	}

	/// Finds the class of the given object
	fn get_object_class<T: 'a + JObject<'a>>(&'a self, obj: &T, _cap: &Capability) -> JavaClass<'a> {
		let cls = unsafe {
			((**self.ptr).GetObjectClass)(self.ptr, obj.get_obj())
		};
		// documentation says, it never returns null
		assert!(cls != 0 as jclass);
		unsafe { JObject::from_unsafe(self, cls) }
	}

	/// Finds the superclass of the given class
	fn get_super_class(&self, sub: &'a JavaClass<'a>, _cap: &Capability) -> Option<JavaClass> {
		let obj = unsafe {
			((**self.ptr).GetSuperclass)(self.ptr, sub.ptr)
		};
		JObject::from(self, obj)
	}

	/// Check if a class can be assigned to another
	fn is_assignable_from(&self, sub: &JavaClass, sup: &JavaClass, _cap: &Capability) -> bool {
		assert!(sub.jvm() == sup.jvm());
		unsafe {
			((**self.ptr).IsAssignableFrom)(self.ptr, sub.ptr, sup.ptr) == JNI_TRUE
		}
	}

	/// Throw a Java exception. The actual exception will be thrown
	/// when the function returns.
	fn throw(&self, obj: &JavaThrowable, cap: Capability) -> Result<Exception, JniError> {
		let (err, _) = unsafe {
			(((**self.ptr).Throw)(self.ptr, obj.ptr), cap)
		};
		// here `cap` is taken, we can't call any Jni methods
		if err == JniError::JNI_OK {
			Ok(Exception::new())
		} else {
			Err(err)
		}
	}

	fn throw_new(&self, cls: &JavaClass, msg: &str, cap: Capability) -> Result<Exception, JniError> {
		let jmsg = JavaChars::new(msg);
		let (err, _) = unsafe {
			(((**self.ptr).ThrowNew)(self.ptr, cls.ptr, jmsg.as_ptr()), cap)
		};
		// here `cap` is taken, we can't call any Jni methods
		if err == JniError::JNI_OK {
			Ok(Exception::new())
		} else {
			Err(err)
		}
	}

	fn exception_check(&self) -> Result<Capability, Exception> {
		let ex = unsafe {
			((**self.ptr).ExceptionCheck)(self.ptr) == JNI_TRUE
		};
		if ex {
			Err(Exception::new())
		} else {
			Ok(Capability::new())
		}
	}

	fn exception_occured(&self) -> Result<Capability, (JavaThrowable, Exception)> {
		let obj = unsafe {
			((**self.ptr).ExceptionOccurred)(self.ptr) as jobject
		};
		if obj == 0 as jthrowable {
			Ok(Capability::new())
		} else {
			Err((JObject::from(self, obj).unwrap(), Exception::new()))
		}
	}

	fn exception_describe(&self, _exn: &Exception) {
		unsafe {
			((**self.ptr).ExceptionDescribe)(self.ptr)
		}
	}

	fn exception_clear(&self, exn: Exception) -> Capability {
		let _  = unsafe {
			((**self.ptr).ExceptionClear)(self.ptr);
			exn
		};
		// here `exn` is taken, so there is no exception
		Capability::new()
	}

	fn fatal_error(&self, msg: &str) -> ! {
		let jmsg = JavaChars::new(msg);
		unsafe {
			((**self.ptr).FatalError)(self.ptr, jmsg.as_ptr());
			unreachable!()
		}
	}

	pub fn push_local_frame(&self, capacity: isize, cap: Capability) -> Result<Capability, (JniError, Exception)> {
		let (err, _) = unsafe {
			(((**self.ptr).PushLocalFrame)(self.ptr, capacity as jint), cap)
		};
		// here `cap` is taken, we can't call any Jni methods
		if err == JniError::JNI_OK {
			Ok(Capability::new())
		} else {
			Err((err, Exception::new()))
		}
	}

	pub fn pop_local_frame_null<T: 'a + JObject<'a>>(&'a self, _cap: &Capability) {
		unsafe {
			((**self.ptr).PopLocalFrame)(self.ptr, 0 as jobject);
		};
	}

	pub fn pop_local_frame<T: 'a + JObject<'a>>(&'a self, result: &'a T, _cap: &Capability) -> T {
		let r = unsafe {
			((**self.ptr).PopLocalFrame)(self.ptr, result.get_obj())
		};
		// documentation says, it never returns null
		assert!(r != 0 as jobject);
		unsafe { JObject::from_unsafe(self, r) }
	}

	fn is_same_object<'b, 'c, T1: 'b + JObject<'b>, T2: 'c + JObject<'c>>(&self, obj1: &T1, obj2: &T2, _cap: &Capability) -> bool {
		assert!(obj1.jvm() == obj2.jvm());
		unsafe {
			((**self.ptr).IsSameObject)(self.ptr, obj1.get_obj(), obj2.get_obj()) == JNI_TRUE
		}
	}

	fn is_null<'b, T: 'b + JObject<'b>>(&self, obj: &T, _cap: &Capability) -> bool {
		unsafe {
			((**self.ptr).IsSameObject)(self.ptr, obj.get_obj(), 0 as jobject) == JNI_TRUE
		}
	}

	fn new_local_ref<T: 'a + JObject<'a>>(&self, lobj: &T, _cap: Capability) -> jobject {
		unsafe { ((**self.ptr).NewLocalRef)(self.ptr, lobj.get_obj()) }
	}

	fn delete_local_ref<T: 'a + JObject<'a>>(&self, gobj: &T, _cap: &Capability) {
		assert!(gobj.ref_type() == RefType::Local);
		unsafe {
			((**self.ptr).DeleteLocalRef)(self.ptr, gobj.get_obj())
		}
	}

	fn new_global_ref<T: 'a + JObject<'a>>(&self, lobj: &T, _cap: Capability) -> jobject {
		unsafe { ((**self.ptr).NewGlobalRef)(self.ptr, lobj.get_obj()) }
	}

	fn delete_global_ref<T: 'a + JObject<'a>>(&self, gobj: &T, _cap: &Capability) {
		assert!(gobj.ref_type() == RefType::Global);
		unsafe {
			((**self.ptr).DeleteGlobalRef)(self.ptr, gobj.get_obj())
		}
	}

	fn new_weak_ref<T: 'a + JObject<'a>>(&self, lobj: &T, _cap: Capability) -> jweak {
		unsafe { ((**self.ptr).NewWeakGlobalRef)(self.ptr, lobj.get_obj()) }
	}

	fn delete_weak_ref<T: 'a + JObject<'a>>(&self, wobj: &T, _cap: &Capability) {
		assert!(wobj.ref_type() == RefType::Weak);
		unsafe {
			((**self.ptr).DeleteWeakGlobalRef)(self.ptr, wobj.get_obj() as jweak)
		}
	}

	pub fn ensure_local_capacity(&self, capacity: isize, cap: Capability) -> Result<Capability, (JniError, Exception)> {
		let (err, _) = unsafe {
			(((**self.ptr).EnsureLocalCapacity)(self.ptr, capacity as jint), cap)
		};
		// here `cap` is taken, we can't call any Jni methods
		if err == JniError::JNI_OK { // here we know, there is no exception
			Ok(Capability::new())
		} else {
			Err((err, Exception::new()))
		}
	}

	fn alloc_object(&self, cls: &JavaClass, cap: Capability) -> JniResult<JavaObject> {
		let (obj, _) = unsafe {
			(((**self.ptr).AllocObject)(self.ptr, cls.ptr), cap)
		};
		// here `cap` is taken, we can't call any Jni methods
		if obj == 0 as jobject {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, obj) }, Capability::new()))
		}
	}

	fn monitor_enter<T: 'a + JObject<'a>>(&self, obj: &T, _cap: &Capability) -> JniError {
		unsafe {
			((**self.ptr).MonitorEnter)(self.ptr, obj.get_obj())
		}
	}

	fn monitor_exit<T: 'a + JObject<'a>>(&self, obj: &T, _cap: &Capability) -> JniError {
		unsafe { ((**self.ptr).MonitorExit)(self.ptr, obj.get_obj()) }
	}

	fn is_instance_of<T: 'a + JObject<'a>>(&self, obj: &T, cls: &JavaClass, _cap: &Capability) -> bool {
		assert!(obj.jvm() == cls.jvm());
		unsafe {
			((**self.ptr).IsInstanceOf)(self.ptr, obj.get_obj(), cls.ptr) == JNI_TRUE
		}
	}

	fn new_string(&'a self, val: &str, cap: Capability) -> JniResult<JavaString<'a>> {
		let jval = JavaChars::new(val);
		let (r, _) = unsafe {
			(((**self.ptr).NewStringUTF)(self.ptr, jval.as_ptr()), cap)
		};
		// here `cap` is taken, we can't call any Jni methods
		if r == 0 as jstring {
			Err(Exception::new())
		} else {
			Ok(( unsafe { JObject::from_unsafe(self, r) }, Capability::new()))
		}
	}

	fn string_len(&self, s: &JavaString, _cap: &Capability) -> usize {
		unsafe {
			((**self.ptr).GetStringLength)(self.ptr, s.ptr) as usize
		}
	}

	fn string_size(&self, s: &JavaString, _cap: &Capability) -> usize {
		unsafe {
			((**self.ptr).GetStringUTFLength)(self.ptr, s.ptr) as usize
		}
	}

	fn string_chars(&self, obj: &'a JavaString<'a>, _cap: &Capability) -> (JavaStringChars, bool) {
		let mut isCopy: jboolean = JNI_FALSE;
		let result = JavaStringChars {
			s: &obj,
			chars: unsafe {
				((**self.ptr).GetStringUTFChars)(self.ptr, obj.ptr, &mut isCopy)
			},
		};
		(result, isCopy == JNI_TRUE)
	}

	fn release_string_chars(&self, s: &mut JavaStringChars<'a>, cap: Capability) {
		let _ = unsafe { ((**self.ptr).ReleaseStringUTFChars)(self.ptr, s.s.ptr, s.chars); cap };
		// here `cap` is taken, we can't call any Jni methods
	}

	fn get_string_region(&self, s: &JavaString<'a>, start: usize, length: usize, cap: Capability) -> JavaChars {
		let mut vec: Vec<u8> = Vec::with_capacity(length + 1);
		unsafe {
			let _ = {
				((**self.ptr).GetStringUTFRegion)(self.ptr, s.ptr, start as jsize, length as jsize, vec.as_mut_ptr() as *mut ::libc::c_char);
				cap
			};
			// here `cap` is taken, we can't call any Jni methods
			vec.set_len(length + 1);
		}
		vec[length] = 0;
		unsafe {
			JavaChars::from_raw_vec(vec)
		}
	}

	fn get_string_unicode_region(&self, s: &JavaString<'a>, start: usize, length: usize, cap: Capability) -> Vec<char> {
		let mut vec: Vec<jchar> = Vec::with_capacity(length);
		unsafe {
			let _ = {
				((**self.ptr).GetStringRegion)(self.ptr, s.ptr, start as jsize, length as jsize, vec.as_mut_ptr());
				cap
			};
			// here `cap` is taken, we can't call any Jni methods
			vec.set_len(length);
		}
		let mut res: Vec<char> = Vec::with_capacity(length);
		for c in &vec {
			match ::std::char::from_u32(*c as u32) {
				None => {},
				Some(c) => res.push(c),
			}
		}
		res
	}

	fn new_direct_byte_buffer(&'a self, capacity: usize, cap: Capability) -> JniResult<JavaDirectByteBuffer<'a>> {
		let mut buf = Vec::with_capacity(capacity);
		unsafe { buf.set_len(capacity) };
		let (obj, _) = unsafe { (((**self.ptr).NewDirectByteBuffer)(self.ptr, buf.as_mut_ptr() as *mut ::libc::c_void, capacity as jlong), cap) };
		// here `cap` is taken, we can't call any Jni methods
		if obj == 0 as jobject {
			Err(Exception::new())
		} else {
			Ok(( unsafe { JavaDirectByteBuffer::from_unsafe_buf(self, obj, buf) }, Capability::new()))
		}
	}

	fn get_direct_byte_buffer_address(&self, buf: &'a JavaDirectByteBuffer<'a>, _cap: &Capability) -> *mut ::libc::c_void {
		unsafe { ((**self.ptr).GetDirectBufferAddress)(self.ptr, buf.get_obj()) }
	}

	fn get_direct_byte_buffer_capacity(&self, buf: &'a JavaDirectByteBuffer<'a>, _cap: &Capability) -> usize {
		unsafe { ((**self.ptr).GetDirectBufferCapacity)(self.ptr, buf.get_obj()) as usize }
	}

	fn array_length<T: 'a + JArrayElem<'a>>(&self, arr: &JavaArray<'a, T>, _cap: &Capability) -> usize {
		unsafe { ((**self.ptr).GetArrayLength)(self.ptr, arr.get_obj() as jarray) as usize }
	}

	fn new_object_array<T: 'a + JArrayElem<'a> + JObject<'a>>(&'a self, len: usize, cls: &JavaClass<'a>, obj: &T, cap: Capability) -> JniResult<JavaArray<'a, T>> {
		let (r, _) = unsafe { (((**self.ptr).NewObjectArray)(self.ptr, len as jsize, cls.get_obj() as jclass, obj.get_obj() as jarray), cap) };
		if r == 0 as jobjectArray {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, r) }, Capability::new()))
		}
	}

	fn get_object_array<T: 'a + JArrayElem<'a> + JObject<'a>>(&'a self, arr: &'a JavaArray<'a, T>, n: usize, cap: Capability) -> JniResult<T> {
		let (r, _) = unsafe { (((**self.ptr).GetObjectArrayElement)(self.ptr, arr.get_obj() as jobjectArray, n as jsize), cap) };
		if r == 0 as jobject {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, r) }, Capability::new()))
		}
	}

	fn set_object_array<T: 'a + JArrayElem<'a> + JObject<'a>>(&'a self, arr: &'a JavaArray<'a, T>, n: usize, val: &T, cap: Capability) {
		let _ = unsafe { ((**self.ptr).SetObjectArrayElement)(self.ptr, arr.get_obj() as jobjectArray, n as jsize, val.get_obj()); cap };
	}

	fn new_boolean_array(&'a self, len: usize, cap: Capability) -> JniResult<JavaArray<'a, <jboolean as JPrimitive>::Type>> {
		let (r, _) = unsafe { (((**self.ptr).NewBooleanArray)(self.ptr, len as jsize), cap) };
		if r == 0 as <jboolean as JPrimitive>::ArrType {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, r) }, Capability::new()))
		}
	}

	fn get_boolean_array(&'a self, arr: &'a JavaArray<'a, <jboolean as JPrimitive>::Type>, n: usize, cap: Capability) -> <jboolean as JPrimitive>::Type {
		let mut val = <jboolean as JPrimitive>::from(false);
		let _ = unsafe { ((**self.ptr).GetBooleanArrayRegion)(self.ptr, arr.get_obj() as <jboolean as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jboolean); cap };
		val.repr()
	}

	fn set_boolean_array(&'a self, arr: &'a JavaArray<'a, <jboolean as JPrimitive>::Type>, n: usize, val: <jboolean as JPrimitive>::Type, cap: Capability) {
		let mut val = <jboolean as JPrimitive>::from(val);
		let _ = unsafe { ((**self.ptr).SetBooleanArrayRegion)(self.ptr, arr.get_obj() as <jboolean as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jboolean); cap };
	}

	fn new_byte_array(&'a self, len: usize, cap: Capability) -> JniResult<JavaArray<'a, <jbyte as JPrimitive>::Type>> {
		let (r, _) = unsafe { (((**self.ptr).NewByteArray)(self.ptr, len as jsize), cap) };
		if r == 0 as <jbyte as JPrimitive>::ArrType {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, r) }, Capability::new()))
		}
	}

	fn get_byte_array(&'a self, arr: &'a JavaArray<'a, <jbyte as JPrimitive>::Type>, n: usize, cap: Capability) -> <jbyte as JPrimitive>::Type {
		let mut val = <jbyte as JPrimitive>::from(0);
		let _ = unsafe { ((**self.ptr).GetByteArrayRegion)(self.ptr, arr.get_obj() as <jbyte as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jbyte); cap };
		val.repr()
	}

	fn set_byte_array(&'a self, arr: &'a JavaArray<'a, <jbyte as JPrimitive>::Type>, n: usize, val: <jbyte as JPrimitive>::Type, cap: Capability) {
		let mut val = <jbyte as JPrimitive>::from(val);
		let _ = unsafe { ((**self.ptr).SetByteArrayRegion)(self.ptr, arr.get_obj() as <jbyte as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jbyte); cap };
	}

	fn new_char_array(&'a self, len: usize, cap: Capability) -> JniResult<JavaArray<'a, <jchar as JPrimitive>::Type>> {
		let (r, _) = unsafe { (((**self.ptr).NewCharArray)(self.ptr, len as jsize), cap) };
		if r == 0 as <jchar as JPrimitive>::ArrType {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, r) }, Capability::new()))
		}
	}

	fn get_char_array(&'a self, arr: &'a JavaArray<'a, <jchar as JPrimitive>::Type>, n: usize, cap: Capability) -> <jchar as JPrimitive>::Type {
		let mut val = <jchar as JPrimitive>::from('\0');
		let _ = unsafe { ((**self.ptr).GetCharArrayRegion)(self.ptr, arr.get_obj() as <jchar as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jchar); cap };
		val.repr()
	}

	fn set_char_array(&'a self, arr: &'a JavaArray<'a, <jchar as JPrimitive>::Type>, n: usize, val: <jchar as JPrimitive>::Type, cap: Capability) {
		let mut val = <jchar as JPrimitive>::from(val);
		let _ = unsafe { ((**self.ptr).SetCharArrayRegion)(self.ptr, arr.get_obj() as <jchar as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jchar); cap };
	}

	fn new_short_array(&'a self, len: usize, cap: Capability) -> JniResult<JavaArray<'a, <jshort as JPrimitive>::Type>> {
		let (r, _) = unsafe { (((**self.ptr).NewShortArray)(self.ptr, len as jsize), cap) };
		if r == 0 as <jshort as JPrimitive>::ArrType {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, r) }, Capability::new()))
		}
	}

	fn get_short_array(&'a self, arr: &'a JavaArray<'a, <jshort as JPrimitive>::Type>, n: usize, cap: Capability) -> <jshort as JPrimitive>::Type {
		let mut val = <jshort as JPrimitive>::from(0);
		let _ = unsafe { ((**self.ptr).GetShortArrayRegion)(self.ptr, arr.get_obj() as <jshort as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jshort); cap };
		val.repr()
	}

	fn set_short_array(&'a self, arr: &'a JavaArray<'a, <jshort as JPrimitive>::Type>, n: usize, val: <jshort as JPrimitive>::Type, cap: Capability) {
		let mut val = <jshort as JPrimitive>::from(val);
		let _ = unsafe { ((**self.ptr).SetShortArrayRegion)(self.ptr, arr.get_obj() as <jshort as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jshort); cap };
	}

	fn new_int_array(&'a self, len: usize, cap: Capability) -> JniResult<JavaArray<'a, <jint as JPrimitive>::Type>> {
		let (r, _) = unsafe { (((**self.ptr).NewIntArray)(self.ptr, len as jsize), cap) };
		if r == 0 as <jint as JPrimitive>::ArrType {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, r) }, Capability::new()))
		}
	}

	fn get_int_array(&'a self, arr: &'a JavaArray<'a, <jint as JPrimitive>::Type>, n: usize, cap: Capability) -> <jint as JPrimitive>::Type {
		let mut val = <jint as JPrimitive>::from(0);
		let _ = unsafe { ((**self.ptr).GetIntArrayRegion)(self.ptr, arr.get_obj() as <jint as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jint); cap };
		val.repr()
	}

	fn set_int_array(&'a self, arr: &'a JavaArray<'a, <jint as JPrimitive>::Type>, n: usize, val: <jint as JPrimitive>::Type, cap: Capability) {
		let mut val = <jint as JPrimitive>::from(val);
		let _ = unsafe { ((**self.ptr).SetIntArrayRegion)(self.ptr, arr.get_obj() as <jint as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jint); cap };
	}

	fn new_long_array(&'a self, len: usize, cap: Capability) -> JniResult<JavaArray<'a, <jlong as JPrimitive>::Type>> {
		let (r, _) = unsafe { (((**self.ptr).NewLongArray)(self.ptr, len as jsize), cap) };
		if r == 0 as <jlong as JPrimitive>::ArrType {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, r) }, Capability::new()))
		}
	}

	fn get_long_array(&'a self, arr: &'a JavaArray<'a, <jlong as JPrimitive>::Type>, n: usize, cap: Capability) -> <jlong as JPrimitive>::Type {
		let mut val = <jlong as JPrimitive>::from(0);
		let _ = unsafe { ((**self.ptr).GetLongArrayRegion)(self.ptr, arr.get_obj() as <jlong as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jlong); cap };
		val.repr()
	}

	fn set_long_array(&'a self, arr: &'a JavaArray<'a, <jlong as JPrimitive>::Type>, n: usize, val: <jlong as JPrimitive>::Type, cap: Capability) {
		let mut val = <jlong as JPrimitive>::from(val);
		let _ = unsafe { ((**self.ptr).SetLongArrayRegion)(self.ptr, arr.get_obj() as <jlong as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jlong); cap };
	}

	fn new_float_array(&'a self, len: usize, cap: Capability) -> JniResult<JavaArray<'a, <jfloat as JPrimitive>::Type>> {
		let (r, _) = unsafe { (((**self.ptr).NewFloatArray)(self.ptr, len as jsize), cap) };
		if r == 0 as <jfloat as JPrimitive>::ArrType {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, r) }, Capability::new()))
		}
	}

	fn get_float_array(&'a self, arr: &'a JavaArray<'a, <jfloat as JPrimitive>::Type>, n: usize, cap: Capability) -> <jfloat as JPrimitive>::Type {
		let mut val = <jfloat as JPrimitive>::from(0.0);
		let _ = unsafe { ((**self.ptr).GetFloatArrayRegion)(self.ptr, arr.get_obj() as <jfloat as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jfloat); cap };
		val.repr()
	}

	fn set_float_array(&'a self, arr: &'a JavaArray<'a, <jfloat as JPrimitive>::Type>, n: usize, val: <jfloat as JPrimitive>::Type, cap: Capability) {
		let mut val = <jfloat as JPrimitive>::from(val);
		let _ = unsafe { ((**self.ptr).SetFloatArrayRegion)(self.ptr, arr.get_obj() as <jfloat as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jfloat); cap };
	}

	fn new_double_array(&'a self, len: usize, cap: Capability) -> JniResult<JavaArray<'a, <jdouble as JPrimitive>::Type>> {
		let (r, _) = unsafe { (((**self.ptr).NewDoubleArray)(self.ptr, len as jsize), cap) };
		if r == 0 as <jdouble as JPrimitive>::ArrType {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, r) }, Capability::new()))
		}
	}

	fn get_double_array(&'a self, arr: &'a JavaArray<'a, <jdouble as JPrimitive>::Type>, n: usize, cap: Capability) -> <jdouble as JPrimitive>::Type {
		let mut val = <jdouble as JPrimitive>::from(0.0);
		let _ = unsafe { ((**self.ptr).GetDoubleArrayRegion)(self.ptr, arr.get_obj() as <jdouble as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jdouble); cap };
		val.repr()
	}

	fn set_double_array(&'a self, arr: &'a JavaArray<'a, <jdouble as JPrimitive>::Type>, n: usize, val: <jdouble as JPrimitive>::Type, cap: Capability) {
		let mut val = <jdouble as JPrimitive>::from(val);
		let _ = unsafe { ((**self.ptr).SetDoubleArrayRegion)(self.ptr, arr.get_obj() as <jdouble as JPrimitive>::ArrType, n as jsize, 1 as jsize, &mut val as *mut jdouble); cap };
	}
}

impl<'a> PartialEq for JavaEnv<'a> {
	fn eq(&self, r: &Self) -> bool {
		self.jvm == r.jvm && self.ptr == r.ptr
	}
}

impl<'a> Eq for JavaEnv<'a> {}

impl<'a> Drop for JavaEnv<'a> {
	fn drop(&mut self) {
		// you can't leave the exception in the air
		match self.exception_check() {
			Ok(_) => (),
			Err(ex) => {
				self.exception_describe(&ex);
				self.exception_clear(ex);
				assert!(false);
			},
		};
		if self.detach {
			self.detach = false;
			let mut jvm: *mut JavaVMImpl = 0 as *mut JavaVMImpl;
			let err = unsafe { ((**self.ptr).GetJavaVM)(self.ptr, &mut jvm) };
			if err != JniError::JNI_OK {
				panic!("GetJavaVM error: {:?}", err);
			}
			let err = unsafe { ((**jvm).DetachCurrentThread)(jvm) };
			if err != JniError::JNI_OK {
				panic!("DetachCurrentThread error: {:?}", err);
			}
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefType {
	Local,
	Global,
	Weak,
}

pub trait JObject<'a>: Eq + Drop {
	fn get_env(&self) -> &'a JavaEnv<'a>;
	fn get_obj(&self) -> jobject;
	fn ref_type(&self) -> RefType;

	fn jvm(&self) -> &'a JavaVM {
		self.get_env().jvm()
	}

	unsafe fn from_unsafe_type(env: &'a JavaEnv<'a>, ptr: jobject, typ: RefType) -> Self;

	unsafe fn from_unsafe(env: &'a JavaEnv<'a>, ptr: jobject) -> Self where Self: Sized {
		Self::from_unsafe_type(env, ptr, RefType::Local)
	}

	fn from(env: &'a JavaEnv<'a>, ptr: jobject) -> Option<Self> where Self: Sized {
		if ptr == 0 as jobject {
			return None;
		}

		Some(unsafe { Self::from_unsafe(env, ptr) })
	}

	fn local(&self, cap: Capability) -> JniResult<Self> where Self: 'a + Sized {
		let r = self.get_env().new_local_ref(self, cap);
		if r == 0 as jobject {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self.get_env(), r) }, Capability::new()))
		}
	}

	fn global(&self, cap: Capability) -> JniResult<Self> where Self: 'a + Sized {
		let r = self.get_env().new_global_ref(self, cap);
		if r == 0 as jobject {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe_type(self.get_env(), r, RefType::Global) }, Capability::new()))
		}
	}

	fn weak(&self, cap: Capability) -> JniResult<Self> where Self: 'a + Sized {
		let r = self.get_env().new_weak_ref(self, cap);
		if r == 0 as jobject {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe_type(self.get_env(), r, RefType::Weak) }, Capability::new()))
		}
	}

	fn get_class(&self, cap: &Capability) -> JavaClass<'a> where Self: 'a + Sized {
		self.get_env().get_object_class(self, cap)
	}

	fn as_jobject(&'a self, cap: Capability) -> JniResult<JavaObject> where Self: Sized {
		let val = self.local(cap);
		if val.is_err() {
			return Err(val.err().unwrap());
		}

		let obj = val.unwrap();
		let r = JavaObject{
			env: obj.0.get_env(),
			ptr: obj.0.get_obj(),
			rtype: obj.0.ref_type()
		};

		Ok((r, obj.1))
	}

	fn is_instance_of(&self, cls: &JavaClass, cap: &Capability) -> bool where Self: 'a + Sized {
		self.get_env().is_instance_of(self, cls, cap)
	}

	fn is_null(&self, cap: &Capability) -> bool where Self: 'a + Sized {
		self.get_env().is_null(self, cap)
	}

	fn monitor(&'a self, cap: &Capability) -> Result<JavaMonitor<'a, Self>, JniError> where Self: Sized {
		JavaMonitor::new(self, cap)
	}
}

#[derive(Debug)]
pub struct JavaMonitor<'a, T: 'a + JObject<'a>> {
	obj: &'a T,
}

impl<'a, T: 'a + JObject<'a>> JavaMonitor<'a, T> {
	fn new(obj: &'a T, cap: &Capability) -> Result<JavaMonitor<'a, T>, JniError> {
		let err = obj.get_env().monitor_enter(obj, cap);
		if err != JniError::JNI_OK {
			Err(err)
		} else {
			Ok(JavaMonitor {
				obj: obj,
			})
		}
	}
}

impl<'a, T: 'a + JObject<'a>> Drop for JavaMonitor<'a, T> {
	fn drop(&mut self) {
		let env = self.obj.get_env();
		match env.exception_check() {
			Ok(cap) => env.monitor_exit(self.obj, &cap),
			Err(_) => panic!("Can't call JNI method with pending exception."),
		};
	}
}

// pub trait JArray<'a, T: 'a + JObject<'a>>: JObject<'a> {}

macro_rules! impl_jobject(
	($cls:ident, $native:ident) => (
		impl<'a> Drop for $cls<'a> {
			fn drop(&mut self) {
				let env = self.get_env();
				match env.exception_check() {
					Ok(cap) => match self.ref_type() {
						RefType::Local => env.delete_local_ref(self, &cap),
						RefType::Global => env.delete_global_ref(self, &cap),
						RefType::Weak => env.delete_weak_ref(self, &cap),
					},
					Err(_) => panic!("Can't call JNI method with pending exception."),
				}
			}
		}

		impl<'a, 'b, R: 'b + JObject<'b>> PartialEq<R> for $cls<'a> {
			fn eq(&self, other: &R) -> bool {
				let env = self.get_env();
				match env.exception_check() {
					Ok(cap) => env.is_same_object(self, other, &cap),
					Err(_) => panic!("Can't call JNI method with pending exception."),
				}
			}
		}

		impl<'a> Eq for $cls<'a> {}

		impl<'a> JObject<'a> for $cls<'a> {
			fn get_env(&self) -> &'a JavaEnv<'a> {
				self.env
			}

			fn get_obj(&self) -> jobject {
				self.ptr as jobject
			}

			fn ref_type(&self) -> RefType {
				self.rtype
			}

			unsafe fn from_unsafe_type(env: &'a JavaEnv<'a>, ptr: jobject, typ: RefType) -> $cls<'a> {
				$cls{
					env: env,
					ptr: ptr as $native,
					rtype: typ,
				}
			}
		}

		impl<'a> JArrayElem<'a> for $cls<'a> {
			fn new_array(env: &'a JavaEnv<'a>, len: usize, val: &$cls<'a>, cap: Capability) -> JniResult<JavaArray<'a, Self>> {
				let cls = val.get_class(&cap);
				env.new_object_array(len, &cls, val, cap)
			}

			fn get(arr: &'a JavaArray<'a, Self>, n: usize, cap: Capability) -> JniResult<Self> {
				arr.env.get_object_array(arr, n, cap)
			}

			fn set(arr: &'a JavaArray<'a, Self>, n: usize, val: &$cls<'a>, cap: Capability) {
				arr.env.set_object_array(arr, n, val, cap)
			}
		}
	);
);

macro_rules! impl_jarray(
	($cls:ident, $native:ident) => (
		impl_jobject!($cls, $native);

		// impl $cls {
		//              pub fn as_jarray(&self) -> JavaArray {
		//                      self.inc_ref();
		//                      JavaArray {
		//                              env: self.get_env(),
		//                              ptr: self.ptr as jarray
		//                      }
		//              }
		// }
	);
);



#[derive(Debug)]
pub struct JavaObject<'a> {
	env: &'a JavaEnv<'a>,
	ptr: jobject,
	rtype: RefType,
}

impl_jobject!(JavaObject, jobject);


#[derive(Debug)]
pub struct JavaClass<'a> {
	env: &'a JavaEnv<'a>,
	ptr: jclass,
	rtype: RefType,
}

impl_jobject!(JavaClass, jclass);

impl<'a> JavaClass<'a> {
	pub fn get_super(&'a self, cap: &Capability) -> Option<JavaClass<'a>> {
		self.env.get_super_class(self, cap)
	}

	pub fn is_assignable_from(&self, cls: &JavaClass<'a>, cap: &Capability) -> bool {
		self.env.is_assignable_from(self, cls, cap)
	}

	pub fn alloc(&'a self, cap: Capability) -> JniResult<JavaObject<'a>> {
		self.env.alloc_object(self, cap)
	}

	pub fn find<'b>(env: &'b JavaEnv<'b>, name: &str, cap: Capability) -> JniResult<JavaClass<'b>> {
		env.find_class(name, cap)
	}

	pub fn define<'b, T: 'b + JObject<'b>>(env: &'b JavaEnv<'b>, name: &str, loader: &T, buf: &[u8], cap: Capability) -> JniResult<JavaClass<'b>> {
		env.define_class(name, loader, buf, cap)
	}
}


#[derive(Debug)]
pub struct JavaThrowable<'a> {
	env: &'a JavaEnv<'a>,
	ptr: jthrowable,
	rtype: RefType,
}

impl_jobject!(JavaThrowable, jthrowable);

impl<'a> JavaThrowable<'a> {
	pub fn throw<'b>(env: &'b JavaEnv<'b>, obj: &JavaThrowable<'b>, cap: Capability) -> Result<Exception, JniError> {
		env.throw(obj, cap)
	}

	pub fn throw_new<'b>(env: &'b JavaEnv<'b>, cls: &JavaClass<'b>, msg: &str, cap: Capability) -> Result<Exception, JniError> {
		env.throw_new(cls, msg, cap)
	}

	pub fn check<'b>(env: &'b JavaEnv<'b>) -> Result<Capability, Exception> {
		env.exception_check()
	}

	pub fn occured<'b>(env: &'b JavaEnv<'b>) -> Result<Capability, (JavaThrowable<'b>, Exception)> {
		env.exception_occured()
	}

	pub fn describe<'b>(env: &'b JavaEnv<'b>, exn: &Exception) {
		env.exception_describe(exn)
	}

	pub fn clear<'b>(env: &'b JavaEnv<'b>, exn: Exception) -> Capability {
		env.exception_clear(exn)
	}

	pub fn fatal_error<'b>(env: &'b JavaEnv<'b>, msg: &str) -> ! {
		env.fatal_error(msg)
	}
}

pub struct JavaString<'a> {
	env: &'a JavaEnv<'a>,
	ptr: jstring,
	rtype: RefType,
}

impl_jobject!(JavaString, jstring);

impl<'a> JavaString<'a> {
	pub fn new<'b>(env: &'b JavaEnv<'b>, val: &str, cap: Capability) -> JniResult<JavaString<'b>> {
		env.new_string(val, cap)
	}

	pub fn len(&self, cap: &Capability) -> usize {
		self.get_env().string_len(self, cap)
	}

	pub fn size(&self, cap: &Capability) -> usize {
		self.get_env().string_size(self, cap)
	}

	pub fn to_str(&self, cap: &Capability) -> Option<string::String> {
		let (chars, _) = self.get_env().string_chars(self, cap);
		chars.to_str()
	}

	pub fn region(&self, start: usize, length: usize, cap: Capability) -> JavaChars {
		self.get_env().get_string_region(self, start, length, cap)
	}

	pub fn as_chars(&self, cap: Capability) -> JavaChars {
		let len = self.len(&cap);
		self.region(0, len, cap)
	}

	pub fn vec_region(&self, start: usize, length: usize, cap: Capability) -> Vec<char> {
		self.get_env().get_string_unicode_region(self, start, length, cap)
	}

	pub fn as_vec(&self, cap: Capability) -> Vec<char> {
		let len = self.len(&cap);
		self.vec_region(0, len, cap)
	}
}

impl<'a> fmt::Debug for JavaString<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self.get_env().exception_check() {
			Ok(cap) => match self.to_str(&cap) { // unsafe!
				None => write!(f, "Invalid JavaString."),
				Some(ref v) => write!(f, "{:?}", v),
			},
			Err(_) => panic!("Can't call JNI method with pending exception."),
		}
	}
}

struct JavaStringChars<'a> {
	s: &'a JavaString<'a>,
	chars: *const ::libc::c_char,
}

impl<'a> Drop for JavaStringChars<'a> {
	fn drop(&mut self) {
		match self.s.get_env().exception_check() {
			Ok(cap) => self.s.get_env().release_string_chars(self, cap),
			Err(_) => panic!("Can't call JNI method with pending exception."),
		};
	}
}

impl<'a> fmt::Debug for JavaStringChars<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self.to_str() {
			None => write!(f, "Invalid JavaStringChars."),
			Some(ref v) => write!(f, "\"{:?}\"", v),
		}
	}
}

impl<'a> JavaStringChars<'a> {
	fn to_str(&self) -> Option<string::String> {
		unsafe { JavaChars::from_raw_vec(CStr::from_ptr(self.chars).to_bytes_with_nul().to_vec()) }.to_string()
	}
}

#[derive(Debug)]
pub struct JavaDirectByteBuffer<'a> {
	env: &'a JavaEnv<'a>,
	ptr: jobject,
	buf: Vec<u8>,
}

impl<'a> JavaDirectByteBuffer<'a> {
	pub fn new<'b>(env: &'b JavaEnv<'b>, capacity: usize, cap: Capability) -> JniResult<JavaDirectByteBuffer<'b>> {
		env.new_direct_byte_buffer(capacity, cap)
	}

	unsafe fn from_unsafe_buf(env: &'a JavaEnv<'a>, ptr: jobject, buf: Vec<u8>) -> JavaDirectByteBuffer<'a> {
		JavaDirectByteBuffer{
			env: env,
			ptr: ptr,
			buf: buf,
		}
	}

	pub fn as_ptr(&self, cap: &Capability) -> *const ::libc::c_void {
		self.env.get_direct_byte_buffer_address(self, cap) as *const ::libc::c_void
	}

	pub fn as_mut_ptr(&mut self, cap: &Capability) -> *mut ::libc::c_void {
		self.env.get_direct_byte_buffer_address(self, cap)
	}

	pub fn capacity(&self, cap: &Capability) -> usize {
		self.env.get_direct_byte_buffer_capacity(self, cap)
	}
}

impl<'a> Drop for JavaDirectByteBuffer<'a> {
	fn drop(&mut self) {
		let env = self.get_env();
		match env.exception_check() {
			Ok(cap) => env.delete_local_ref(self, &cap),
			Err(_) => panic!("Can't call JNI method with pending exception."),
		}
	}
}

impl<'a, 'b, R: 'b + JObject<'b>> PartialEq<R> for JavaDirectByteBuffer<'a> {
	fn eq(&self, other: &R) -> bool {
		let env = self.get_env();
		match env.exception_check() {
			Ok(cap) => env.is_same_object(self, other, &cap),
			Err(_) => panic!("Can't call JNI method with pending exception."),
		}
	}
}

impl<'a> Eq for JavaDirectByteBuffer<'a> {}

impl<'a> JObject<'a> for JavaDirectByteBuffer<'a> {
	fn get_env(&self) -> &'a JavaEnv<'a> {
		self.env
	}

	fn get_obj(&self) -> jobject {
		self.ptr
	}

	fn ref_type(&self) -> RefType {
		RefType::Local
	}

	unsafe fn from_unsafe_type(env: &'a JavaEnv<'a>, ptr: jobject, typ: RefType) -> JavaDirectByteBuffer<'a> {
		assert!(typ == RefType::Local);
		JavaDirectByteBuffer{
			env: env,
			ptr: ptr,
			buf: Vec::new(),
		}
	}
}

impl<'a> JArrayElem<'a> for JavaDirectByteBuffer<'a> {
	fn new_array(env: &'a JavaEnv<'a>, len: usize, val: &JavaDirectByteBuffer<'a>, cap: Capability) -> JniResult<JavaArray<'a, Self>> {
		let cls = val.get_class(&cap);
		env.new_object_array(len, &cls, val, cap)
	}

	fn get(arr: &'a JavaArray<'a, Self>, n: usize, cap: Capability) -> JniResult<Self> {
		arr.env.get_object_array(arr, n, cap)
	}

	fn set(arr: &'a JavaArray<'a, Self>, n: usize, val: &JavaDirectByteBuffer<'a>, cap: Capability) {
		arr.env.set_object_array(arr, n, val, cap)
	}
}

pub trait JArrayElem<'a> {
	fn new_array(env: &'a JavaEnv<'a>, len: usize, val: &Self, cap: Capability) -> JniResult<JavaArray<'a, Self>>;
	fn get(arr: &'a JavaArray<'a, Self>, n: usize, cap: Capability) -> JniResult<Self>;
	fn set(arr: &'a JavaArray<'a, Self>, n: usize, val: &Self, cap: Capability);
}

impl<'a> JArrayElem<'a> for <jboolean as JPrimitive>::Type {
	fn new_array(env: &'a JavaEnv<'a>, len: usize, _val: &Self, cap: Capability) -> JniResult<JavaArray<'a, Self>> {
		env.new_boolean_array(len, cap)
	}

	fn get(arr: &'a JavaArray<'a, Self>, n: usize, cap: Capability) -> JniResult<Self> {
		let r = arr.env.get_boolean_array(arr, n, cap);
		match arr.env.exception_check() {
			Ok(cap) => Ok((r, cap)),
			Err(ex) => Err(ex),
		}
	}

	fn set(arr: &'a JavaArray<'a, Self>, n: usize, val: &Self, cap: Capability) {
		arr.env.set_boolean_array(arr, n, *val, cap)
	}
}

impl<'a> JArrayElem<'a> for <jbyte as JPrimitive>::Type {
	fn new_array(env: &'a JavaEnv<'a>, len: usize, _val: &Self, cap: Capability) -> JniResult<JavaArray<'a, Self>> {
		env.new_byte_array(len, cap)
	}

	fn get(arr: &'a JavaArray<'a, Self>, n: usize, cap: Capability) -> JniResult<Self> {
		let r = arr.env.get_byte_array(arr, n, cap);
		match arr.env.exception_check() {
			Ok(cap) => Ok((r, cap)),
			Err(ex) => Err(ex),
		}
	}

	fn set(arr: &'a JavaArray<'a, Self>, n: usize, val: &Self, cap: Capability) {
		arr.env.set_byte_array(arr, n, *val, cap)
	}
}

impl<'a> JArrayElem<'a> for <jchar as JPrimitive>::Type {
	fn new_array(env: &'a JavaEnv<'a>, len: usize, _val: &Self, cap: Capability) -> JniResult<JavaArray<'a, Self>> {
		env.new_char_array(len, cap)
	}

	fn get(arr: &'a JavaArray<'a, Self>, n: usize, cap: Capability) -> JniResult<Self> {
		let r = arr.env.get_char_array(arr, n, cap);
		match arr.env.exception_check() {
			Ok(cap) => Ok((r, cap)),
			Err(ex) => Err(ex),
		}
	}

	fn set(arr: &'a JavaArray<'a, Self>, n: usize, val: &Self, cap: Capability) {
		arr.env.set_char_array(arr, n, *val, cap)
	}
}

impl<'a> JArrayElem<'a> for <jshort as JPrimitive>::Type {
	fn new_array(env: &'a JavaEnv<'a>, len: usize, _val: &Self, cap: Capability) -> JniResult<JavaArray<'a, Self>> {
		env.new_short_array(len, cap)
	}

	fn get(arr: &'a JavaArray<'a, Self>, n: usize, cap: Capability) -> JniResult<Self> {
		let r = arr.env.get_short_array(arr, n, cap);
		match arr.env.exception_check() {
			Ok(cap) => Ok((r, cap)),
			Err(ex) => Err(ex),
		}
	}

	fn set(arr: &'a JavaArray<'a, Self>, n: usize, val: &Self, cap: Capability) {
		arr.env.set_short_array(arr, n, *val, cap)
	}
}

impl<'a> JArrayElem<'a> for <jint as JPrimitive>::Type {
	fn new_array(env: &'a JavaEnv<'a>, len: usize, _val: &Self, cap: Capability) -> JniResult<JavaArray<'a, Self>> {
		env.new_int_array(len, cap)
	}

	fn get(arr: &'a JavaArray<'a, Self>, n: usize, cap: Capability) -> JniResult<Self> {
		let r = arr.env.get_int_array(arr, n, cap);
		match arr.env.exception_check() {
			Ok(cap) => Ok((r, cap)),
			Err(ex) => Err(ex),
		}
	}

	fn set(arr: &'a JavaArray<'a, Self>, n: usize, val: &Self, cap: Capability) {
		arr.env.set_int_array(arr, n, *val, cap)
	}
}

impl<'a> JArrayElem<'a> for <jlong as JPrimitive>::Type {
	fn new_array(env: &'a JavaEnv<'a>, len: usize, _val: &Self, cap: Capability) -> JniResult<JavaArray<'a, Self>> {
		env.new_long_array(len, cap)
	}

	fn get(arr: &'a JavaArray<'a, Self>, n: usize, cap: Capability) -> JniResult<Self> {
		let r = arr.env.get_long_array(arr, n, cap);
		match arr.env.exception_check() {
			Ok(cap) => Ok((r, cap)),
			Err(ex) => Err(ex),
		}
	}

	fn set(arr: &'a JavaArray<'a, Self>, n: usize, val: &Self, cap: Capability) {
		arr.env.set_long_array(arr, n, *val, cap)
	}
}

impl<'a> JArrayElem<'a> for <jfloat as JPrimitive>::Type {
	fn new_array(env: &'a JavaEnv<'a>, len: usize, _val: &Self, cap: Capability) -> JniResult<JavaArray<'a, Self>> {
		env.new_float_array(len, cap)
	}

	fn get(arr: &'a JavaArray<'a, Self>, n: usize, cap: Capability) -> JniResult<Self> {
		let r = arr.env.get_float_array(arr, n, cap);
		match arr.env.exception_check() {
			Ok(cap) => Ok((r, cap)),
			Err(ex) => Err(ex),
		}
	}

	fn set(arr: &'a JavaArray<'a, Self>, n: usize, val: &Self, cap: Capability) {
		arr.env.set_float_array(arr, n, *val, cap)
	}
}

impl<'a> JArrayElem<'a> for <jdouble as JPrimitive>::Type {
	fn new_array(env: &'a JavaEnv<'a>, len: usize, _val: &Self, cap: Capability) -> JniResult<JavaArray<'a, Self>> {
		env.new_double_array(len, cap)
	}

	fn get(arr: &'a JavaArray<'a, Self>, n: usize, cap: Capability) -> JniResult<Self> {
		let r = arr.env.get_double_array(arr, n, cap);
		match arr.env.exception_check() {
			Ok(cap) => Ok((r, cap)),
			Err(ex) => Err(ex),
		}
	}

	fn set(arr: &'a JavaArray<'a, Self>, n: usize, val: &Self, cap: Capability) {
		arr.env.set_double_array(arr, n, *val, cap)
	}
}

pub struct JavaArray<'a, T: 'a + JArrayElem<'a>> {
	env: &'a JavaEnv<'a>,
	ptr: jobjectArray,
	rtype: RefType,
	phantom: PhantomData<T>,
}

impl<'a, T: 'a + JArrayElem<'a>> JavaArray<'a, T> {
	pub fn new(env: &'a JavaEnv<'a>, len: usize, obj: &T, cap: Capability) -> JniResult<JavaArray<'a, T>> {
		T::new_array(env, len, obj, cap)
	}

	pub fn len(&self, cap: &Capability) -> usize {
		self.get_env().array_length(self, cap)
	}

	pub fn get(&'a self, n: usize, cap: Capability) -> JniResult<T> {
		T::get(self, n, cap)
	}

	pub fn set(&'a self, n: usize, val: &T, cap: Capability) {
		T::set(self, n, val, cap)
	}
}

impl<'a, T: 'a + JArrayElem<'a>> Drop for JavaArray<'a, T> {
	fn drop(&mut self) {
		let env = self.get_env();
		match env.exception_check() {
			Ok(cap) => match self.ref_type() {
				RefType::Local => env.delete_local_ref(self, &cap),
				RefType::Global => env.delete_global_ref(self, &cap),
				RefType::Weak => env.delete_weak_ref(self, &cap),
			},
			Err(_) => panic!("Can't call JNI method with pending exception."),
		}
	}
}

impl<'a, 'b, T: 'a + JArrayElem<'a>, R: 'b + JObject<'b>> PartialEq<R> for JavaArray<'a, T> {
	fn eq(&self, other: &R) -> bool {
		let env = self.get_env();
		match env.exception_check() {
			Ok(cap) => env.is_same_object(self, other, &cap),
			Err(_) => panic!("Can't call JNI method with pending exception."),
		}
	}
}

impl<'a, T: 'a + JArrayElem<'a>> Eq for JavaArray<'a, T> {}

impl<'a, T: 'a + JArrayElem<'a>> JObject<'a> for JavaArray<'a, T> {
	fn get_env(&self) -> &'a JavaEnv<'a> {
		self.env
	}

	fn get_obj(&self) -> jobject {
		self.ptr as jobject
	}

	fn ref_type(&self) -> RefType {
		self.rtype
	}

	unsafe fn from_unsafe_type(env: &'a JavaEnv<'a>, ptr: jobject, typ: RefType) -> JavaArray<'a, T> {
		JavaArray{
			env: env,
			ptr: ptr as jarray,
			rtype: typ,
			phantom: PhantomData::<T>,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use super::super::native::*;

	#[test]
	fn test_JavaVMOption() {
		for s in &["", "-Xcheck:jni", "a"] {
			let opt = JavaVMOption::new(s);
			assert!(opt.extraInfo == 0 as *const ::libc::c_void);
			assert!(opt.optionString == *s);
			assert!(opt == *s);
		}
	}

	#[test]
	fn test_JavaVMInitArgs() {
		let args = JavaVMInitArgs::new(
			JniVersion::JNI_VERSION_1_8,
			&[JavaVMOption::new("-Xcheck:jni"), JavaVMOption::new("-ea")],
			false
		);
		assert!(!args.ignoreUnrecognized);
		assert!(args.version == JniVersion::JNI_VERSION_1_8);
		assert!(args.options.len() == 2);
		assert!(args.options[0] == "-Xcheck:jni");
		assert!(args.options == ["-Xcheck:jni", "-ea"]);
	}

	fn test_JavaEnv(jvm: &JavaVM) {
		let (env, cap) = jvm.get_env().unwrap();
		assert!(env.version(&cap) >= jvm.version());

		let (cls, cap) = JavaClass::find(&env, "java/lang/String", cap).unwrap();
		let (obj, cap) = cls.alloc(cap).unwrap();
		let cls1 = obj.get_class(&cap);
		assert!(cls1 == cls);
		let (sobj, cap) = JavaString::new(&env, "hi!", cap).unwrap();

		assert!(cls1 != sobj);
		let scls = sobj.get_class(&cap);
		assert!(scls == cls1);
		assert!(scls == cls);
		assert!(cls1 == scls);
		assert!(cls == scls);
		assert!(scls.get_obj() != 0 as jobject);
		let cap = env.exception_check().unwrap();

		let cls = env.find_class("java/lang/String1", cap);
		assert!(cls.is_err());
		let tex = env.exception_check();
		assert!(tex.is_err());
		let ex = cls.err().unwrap();
		let _ = env.exception_clear(ex);
	}

	#[test]
	fn test_JavaVM() {
		use std::thread;

		let jvm = JavaVM::new(
			JavaVMInitArgs::new(
				JniVersion::JNI_VERSION_1_8,
				&[/*JavaVMOption::new("-Xcheck:jni"), JavaVMOption::new("-verbose:jni")*/],
				false,
			)
		).unwrap();
		assert!(jvm.version() == JniVersion::JNI_VERSION_1_8);

		let created = JavaVM::created().unwrap();
		assert!(created.len() == 1);
		assert!(created[0] == jvm);

		test_JavaEnv(&jvm);
		test_JavaEnv(&jvm);

		let t1 = thread::scoped(|| {
			test_JavaEnv(&jvm);
		});

		let t2 = thread::scoped(|| {
			test_JavaEnv(&jvm);
		});

		let t3 = thread::scoped(|| {
			test_JavaEnv(&jvm);
		});

		let _ = t1.join();
		let _ = t2.join();
		let _ = t3.join();
	}
}
