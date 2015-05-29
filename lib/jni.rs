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
	pub fn new() -> Capability {
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
	pub fn new() -> Exception {
		Exception { _cap: () }
	}
}

pub type JniResult<T> = Result<(T, Capability), Exception>;

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
		let mut res = JavaVMInitArgs {
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
					JavaVMOptionImpl {
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
	pub fn define_class<T: 'a + JObject<'a>>(&self, name: &str, loader: &T, buf: &[u8], cap: Capability) -> JniResult<JavaClass> {
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
	pub fn find_class(&self, name: &str, cap: Capability) -> JniResult<JavaClass> {
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
	pub fn get_object_class<T: 'a + JObject<'a>>(&'a self, obj: &T, _cap: &Capability) -> JavaClass<'a> {
		let cls = unsafe {
			((**self.ptr).GetObjectClass)(self.ptr, obj.get_obj())
		};
		// documentation says, it never returns null
		assert!(cls != 0 as jclass);
		unsafe { JObject::from_unsafe(self, cls) }
	}

	/// Finds the superclass of the given class
	pub fn get_super_class(&self, sub: &'a JavaClass<'a>, _cap: &Capability) -> Option<JavaClass> {
		let obj = unsafe {
			((**self.ptr).GetSuperclass)(self.ptr, sub.ptr)
		};
		JObject::from(self, obj)
	}

	/// Check if a class can be assigned to another
	pub fn is_assignable_from(&self, sub: &JavaClass, sup: &JavaClass, _cap: &Capability) -> bool {
		unsafe {
			((**self.ptr).IsAssignableFrom)(self.ptr, sub.ptr, sup.ptr) == JNI_TRUE
		}
	}

	/// Throw a Java exception. The actual exception will be thrown
	/// when the function returns.
	pub fn throw(&self, obj: &JavaThrowable, cap: Capability) -> Result<Exception, JniError> {
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

	pub fn throw_new(&self, clazz: &JavaClass, msg: &str, cap: Capability) -> Result<Exception, JniError> {
		let jmsg = JavaChars::new(msg);
		let (err, _) = unsafe {
			(((**self.ptr).ThrowNew)(self.ptr, clazz.ptr, jmsg.as_ptr()), cap)
		};
		// here `cap` is taken, we can't call any Jni methods
		if err == JniError::JNI_OK {
			Ok(Exception::new())
		} else {
			Err(err)
		}
	}

	pub fn exception_check(&self) -> Result<Capability, Exception> {
		let ex = unsafe {
			((**self.ptr).ExceptionCheck)(self.ptr) == JNI_TRUE
		};
		if ex {
			Err(Exception::new())
		} else {
			Ok(Capability::new())
		}
	}

	pub fn exception_occured(&self) -> Result<Capability, (JavaThrowable, Exception)> {
		let obj = unsafe {
			((**self.ptr).ExceptionOccurred)(self.ptr) as jobject
		};
		if obj == 0 as jthrowable {
			Ok(Capability::new())
		} else {
			Err((JObject::from(self, obj).unwrap(), Exception::new()))
		}
	}

	pub fn exception_describe(&self, _exn: &Exception) {
		unsafe {
			((**self.ptr).ExceptionDescribe)(self.ptr)
		}
	}

	pub fn exception_clear(&self, exn: Exception) -> Capability {
		let _  = unsafe {
			((**self.ptr).ExceptionClear)(self.ptr);
			exn
		};
		// here `exn` is taken, so there is no exception
		Capability::new()
	}

	pub fn fatal_error(&self, msg: &str) -> ! {
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

	// TODO: 'b MUST be == 'a
	pub fn is_same_object<'b, 'c, T1: 'b + JObject<'b>, T2: 'c + JObject<'c>>(&self, obj1: &T1, obj2: &T2) -> bool {
		assert!(obj1.get_env().jvm() == obj2.get_env().jvm());
		unsafe {
			((**self.ptr).IsSameObject)(self.ptr, obj1.get_obj(), obj2.get_obj()) == JNI_TRUE
		}
	}

	pub fn is_null<'b, T: 'b + JObject<'b>>(&self, obj1: &T) -> bool {
		unsafe {
			((**self.ptr).IsSameObject)(self.ptr, obj1.get_obj(), 0 as jobject) == JNI_TRUE
		}
	}

	unsafe fn new_local_ref<T: 'a + JObject<'a>>(&self, lobj: &T) -> jobject {
		((**self.ptr).NewLocalRef)(self.ptr, lobj.get_obj())
	}

	fn delete_local_ref<T: 'a + JObject<'a>>(&self, gobj: &T) {
		assert!(gobj.ref_type() == RefType::Local);
		unsafe {
			((**self.ptr).DeleteLocalRef)(self.ptr, gobj.get_obj())
		}
	}

	unsafe fn new_global_ref<T: 'a + JObject<'a>>(&self, lobj: &T) -> jobject {
		((**self.ptr).NewGlobalRef)(self.ptr, lobj.get_obj())
	}

	fn delete_global_ref<T: 'a + JObject<'a>>(&self, gobj: &T) {
		assert!(gobj.ref_type() == RefType::Global);
		unsafe {
			((**self.ptr).DeleteGlobalRef)(self.ptr, gobj.get_obj())
		}
	}

	unsafe fn new_weak_ref<T: 'a + JObject<'a>>(&self, lobj: &T) -> jweak {
		((**self.ptr).NewWeakGlobalRef)(self.ptr, lobj.get_obj())
	}

	fn delete_weak_ref<T: 'a + JObject<'a>>(&self, wobj: &T) {
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

	pub fn alloc_object(&self, clazz: &JavaClass, cap: Capability) -> JniResult<JavaObject> {
		let (obj, _) = unsafe {
			(((**self.ptr).AllocObject)(self.ptr, clazz.ptr), cap)
		};
		// here `cap` is taken, we can't call any Jni methods
		if obj == 0 as jobject {
			Err(Exception::new())
		} else {
			Ok((unsafe { JObject::from_unsafe(self, obj) }, Capability::new()))
		}
	}

	pub fn monitor_enter<T: 'a + JObject<'a>>(&self, obj: &T, _cap: &Capability) -> JniError {
		unsafe {
			((**self.ptr).MonitorEnter)(self.ptr, obj.get_obj())
		}
	}

	pub fn monitor_exit<T: 'a + JObject<'a>>(&self, obj: &T, cap: Capability) -> Result<Capability, (JniError, Exception)> {
		let (err, _) = unsafe {
			(((**self.ptr).MonitorExit)(self.ptr, obj.get_obj()), cap)
		};
		// here `cap` is taken, we can't call any Jni methods
		if err == JniError::JNI_OK {
			Ok(Capability::new())
		} else {
			Err((err, Exception::new()))
		}
	}

	pub fn is_instance_of<T: 'a + JObject<'a>>(&self, obj: &T, clazz: &JavaClass, _cap: &Capability) -> bool {
		unsafe {
			((**self.ptr).IsInstanceOf)(self.ptr, obj.get_obj(), clazz.ptr) == JNI_TRUE
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
}

impl<'a> PartialEq for JavaEnv<'a> {
	fn eq(&self, r: &Self) -> bool {
		assert!(self.jvm == r.jvm); // can't compare envs from different VMs.
		self.ptr == r.ptr
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

pub trait JObject<'a>: Drop {
	fn get_env(&self) -> &'a JavaEnv<'a>;
	fn get_obj(&self) -> jobject;
	fn ref_type(&self) -> RefType;

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
		unsafe {
			let (r, _) = (self.get_env().new_local_ref(self), cap);
			// here `cap` is taken, we can't call any Jni methods
			if r == 0 as jobject {
				Err(Exception::new())
			} else {
				Ok((JObject::from_unsafe_type(self.get_env(), r, RefType::Local), Capability::new()))
			}
		}
	}

	fn global(&self, cap: Capability) -> JniResult<Self> where Self: 'a + Sized {
		unsafe {
			let (r, _) = (self.get_env().new_global_ref(self), cap);
			// here `cap` is taken, we can't call any Jni methods
			if r == 0 as jobject {
				Err(Exception::new())
			} else {
				Ok((JObject::from_unsafe_type(self.get_env(), r, RefType::Global), Capability::new()))
			}
		}
	}

	fn weak(&self, cap: Capability) -> JniResult<Self> where Self: 'a + Sized {
		unsafe {
			let (r, _) = (self.get_env().new_weak_ref(self), cap);
			// here `cap` is taken, we can't call any Jni methods
			if r == 0 as jobject {
				Err(Exception::new())
			} else {
				Ok((JObject::from_unsafe_type(self.get_env(), r, RefType::Weak), Capability::new()))
			}
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
		let r = JavaObject {
			env: obj.0.get_env(),
			ptr: obj.0.get_obj(),
			rtype: obj.0.ref_type()
		};

		Ok((r, obj.1))
	}

	fn is_instance_of(&self, clazz: &JavaClass, cap: &Capability) -> bool where Self: 'a + Sized {
		self.get_env().is_instance_of(self, clazz, cap)
	}

	fn is_null(&self) -> bool where Self: 'a + Sized {
		self.get_env().is_null(self)
	}
}

// pub trait JArray<'a, T: 'a + JObject<'a>>: JObject<'a> {}

macro_rules! impl_jobject(
	($cls:ident, $native:ident) => (
		impl<'a> Drop for $cls<'a> {
			fn drop(&mut self) {
				let env = self.get_env();
				match self.ref_type() {
					RefType::Local => env.delete_local_ref(self),
					RefType::Global => env.delete_global_ref(self),
					RefType::Weak => env.delete_weak_ref(self),
				}
			}
		}

		// TODO: 'b MUST be == 'a
		impl<'a, 'b, R: 'b + JObject<'b>> PartialEq<R> for $cls<'a> {
			fn eq(&self, other: &R) -> bool {
				self.get_env().is_same_object(self, other)
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
				$cls {
					env: env,
					ptr: ptr as $native,
					rtype: typ,
				}
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

	pub fn alloc(&'a self, cap: Capability) -> JniResult<JavaObject<'a>> {
		self.env.alloc_object(self, cap)
	}

	pub fn find<'b>(env: &'b JavaEnv<'b>, name: &str, cap: Capability) -> JniResult<JavaClass<'b>> {
		env.find_class(name, cap)
	}
}


#[derive(Debug)]
pub struct JavaThrowable<'a> {
	env: &'a JavaEnv<'a>,
	ptr: jthrowable,
	rtype: RefType,
}

impl_jobject!(JavaThrowable, jthrowable);

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
		let mut vec: Vec<u8> = Vec::with_capacity(length + 1);
		unsafe {
			let _ = {
				((**self.get_env().ptr).GetStringUTFRegion)(self.get_env().ptr, self.ptr, start as jsize, length as jsize, vec.as_mut_ptr() as *mut ::libc::c_char);
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
}

impl<'a> fmt::Debug for JavaString<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self.get_env().exception_check() {
			Ok(cap) => match self.to_str(&cap) { // unsafe!
				None => write!(f, "Invalid JavaString."),
				Some(ref v) => write!(f, "\"{:?}\"", v),
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
			Ok(cap) => {
				let _ = unsafe { ((**self.s.env.ptr).ReleaseStringUTFChars)(self.s.env.ptr, self.s.ptr, self.chars); cap };
			},
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

// For future
trait JavaPrimitive {}

impl JavaPrimitive for jboolean {}
impl JavaPrimitive for jbyte {}
impl JavaPrimitive for jchar {}
impl JavaPrimitive for jshort {}
impl JavaPrimitive for jint {}
impl JavaPrimitive for jlong {}
impl JavaPrimitive for jfloat {}
impl JavaPrimitive for jdouble {}

pub struct JavaArray<'a, T: 'a + JObject<'a>> {
	env: &'a JavaEnv<'a>,
	ptr: jarray,
	rtype: RefType,
	phantom: PhantomData<T>,
}

impl<'a, T: 'a + JObject<'a>> Drop for JavaArray<'a, T> {
	fn drop(&mut self) {
		let env = self.get_env();
		match self.ref_type() {
			RefType::Local => env.delete_local_ref(self),
			RefType::Global => env.delete_global_ref(self),
			RefType::Weak => env.delete_weak_ref(self),
		}
	}
}

impl<'a, T: 'a + JObject<'a>, R: 'a + JObject<'a>> PartialEq<R> for JavaArray<'a, T> {
	fn eq(&self, other: &R) -> bool {
		self.get_env().is_same_object(self, other)
	}
}

impl<'a, T: 'a + JObject<'a>> Eq for JavaArray<'a, T> {}

impl<'a, T: 'a + JObject<'a>> JObject<'a> for JavaArray<'a, T> {
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
		JavaArray {
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

		let _ = t1.join();
		let _ = t2.join();
	}
}
