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
//! solve this,  this interface  converts `null`  to `None`  and other
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
use ::std::ffi::CString;
use ::std::marker::PhantomData;

use super::native::*;

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
pub struct Exception {
	_cap: ()
}

impl Exception {
	pub fn new() -> Exception {
		Exception { _cap: () }
	}
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
		JavaVMInitArgs{
			version: version,
			options: options.to_vec(),
			ignoreUnrecognized: ignoreUnrecognized
		}
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
	name: String,
}

impl JavaVM {
	/// Creates a Java Virtual Machine.
	/// The JVM will automatically be destroyed when the object goes out of scope.
	pub fn new(args: JavaVMInitArgs, name: &str) -> Result<(JavaVM, Capability), JniError> {
		use ::std::borrow::ToOwned;
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
				ignoreUnrecognized: args.ignoreUnrecognized as jboolean
			};

			let res = JNI_CreateJavaVM(&mut jvm, &mut env, &mut argsImpl);

			(res, jvm)
		};

		match res {
			JniError::JNI_OK => {
				let r = JavaVM{
					ptr: jvm,
					version: args.version,
					name: name.to_owned(),
				};
				Ok((r, Capability::new()))
			}
			_ => Err(res)
		}
	}

	pub fn version(&self) -> JniVersion {
		return self.version
	}

	pub fn get_env(&mut self) -> Result<JavaEnv, JniError> {
		unsafe {
			let ref jni = **self.ptr;
			self.get_env_gen(jni.AttachCurrentThread)
		}
	}

	pub fn get_env_daemon(&mut self) -> Result<JavaEnv, JniError> {
		unsafe {
			let ref jni = **self.ptr;
			self.get_env_gen(jni.AttachCurrentThreadAsDaemon)
		}
	}

	pub fn detach_current_thread(&mut self) -> bool {
		unsafe {
			let ref jni = **self.ptr;
			(jni.DetachCurrentThread)(self.ptr) == JniError::JNI_OK
		}
	}

	unsafe fn get_env_gen(&mut self, fun: extern "C" fn(vm: *mut JavaVMImpl, penv: &mut *mut JNIEnvImpl, args: *mut JavaVMAttachArgsImpl) -> JniError) -> Result<JavaEnv, JniError> {
		let mut env: *mut JNIEnvImpl = 0 as *mut JNIEnvImpl;
		let res = ((**self.ptr).GetEnv)(self.ptr, &mut env, self.version());
		match res {
			JniError::JNI_OK => Ok(JavaEnv{
				ptr: &mut *env,
				phantom: PhantomData,
			}),
			JniError::JNI_EDETACHED => {
				let mut attachArgs = JavaVMAttachArgsImpl{
					version: self.version(),
					name: self.name.as_ptr() as *const ::libc::c_char,
					group: 0 as jobject
				};
				let res = fun(self.ptr, &mut env, &mut attachArgs);
				match res {
					JniError::JNI_OK => Ok(JavaEnv{
						ptr: &mut *env,
						phantom: PhantomData,
					}),
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

impl Drop for JavaVM {
	fn drop(&mut self) {
		unsafe {
			let err = self.destroy_java_vm();
			if err != JniError::JNI_OK {
				panic!("DestroyJavaVM error: {:?}", err);
			}
		}
	}
}

/// Represents an environment pointer used by the JNI.
/// Serves as an upper bound to the lifetime of all local refs
/// created by this binding.
///
/// TODO: allow for global/weak refs to outlive their env.
#[derive(Debug, Clone)]
#[allow(raw_pointer_derive)]
pub struct JavaEnv<'a> {
	ptr: *mut JNIEnvImpl,
	phantom: PhantomData<&'a JavaVM>,
}

impl<'a> JavaEnv<'a> {
	/// Gets the version of the JVM (mightt be bigger, than the JavaVM args version, but not less)
	pub fn version(&self, _cap: &Capability) -> JniVersion {
		unsafe {
			mem::transmute(((**self.ptr).GetVersion)(self.ptr))
		}
	}

	pub fn ptr(&self) -> *mut JNIEnvImpl {
		self.ptr
	}

	/// Defines a Java class from a name, ClassLoader, buffer, and length
	pub fn define_class<'b, T: 'b + JObject<'b>>(&self, name: &JavaChars,
												 loader: &T, buf: &[u8],
												 cap: Capability) -> Result<(JavaClass, Capability), Exception> {
		unsafe {
			JObject::from_unless_null(
				self.clone(),
				((**self.ptr).DefineClass)(
					self.ptr,
					name.as_ptr() as *const ::libc::c_char,
					loader.get_obj(),
					buf.as_ptr() as *const jbyte,
					buf.len() as jsize
						),
				cap)
		}
	}

	/// Takes a string and returns a Java class if successfull.
	/// Returns `Err` on failure.
	pub fn find_class(&self, name: &JavaChars, cap: Capability) -> ThisResult<JavaClass> {
		unsafe {
			JObject::from_unless_null(
				self.clone(),
				((**self.ptr).FindClass)(self.ptr, name.as_ptr()),
				cap)
		}
	}

	/// Finds the superclass of the given class
	pub fn get_super_class<'b>(&'b self, sub: &'b JavaClass<'b>, cap: &Capability) -> Option<JavaClass> {
		sub.get_super(cap)
	}

	/// Check if a class can be assigned to another
	pub fn is_assignable_from(&self, sub: &JavaClass, sup: &JavaClass, _cap: &Capability) -> bool {
		unsafe {
			((**self.ptr).IsAssignableFrom)(self.ptr, sub.ptr, sup.ptr) != 0
		}
	}

	/// Throw a Java exception. The actual exception will be thrown
	/// when the function returns.
	pub fn throw(&self, obj: &JavaThrowable, _cap: Capability) -> (bool, Exception)  {
		unsafe {
			(((**self.ptr).Throw)(self.ptr, obj.ptr) == JniError::JNI_OK, Exception::new())
		}
	}

	pub fn throw_new(&self, clazz: &JavaClass, msg: &JavaChars, _cap: Capability) -> (bool, Exception) {
		unsafe {
			(((**self.ptr).ThrowNew)(self.ptr, clazz.ptr, msg.as_ptr() as *const ::libc::c_char) == JniError::JNI_OK,
			 Exception::new())
		}
	}

	pub fn exception_occured(&self) -> Option<JavaThrowable> {
		let ptr = unsafe {
			((**self.ptr).ExceptionOccurred)(self.ptr) as jobject
		};
		if ptr.is_null() {
			None
		} else {
			Some(JavaThrowable {
				env: self.clone(),
				ptr: ptr as jclass,
				rtype: RefType::Local,
			})
		}
	}

	pub fn exception_describe(&self) {
		unsafe {
			((**self.ptr).ExceptionDescribe)(self.ptr)
		}
	}

	pub fn exception_clear(&self, _exn: Exception) -> Capability {
		unsafe {
			((**self.ptr).ExceptionClear)(self.ptr)
		}
		Capability::new()
	}

	pub fn fatal_error(&self, msg: &JavaChars, _cap: &Capability) -> ! {
		unsafe {
			((**self.ptr).FatalError)(self.ptr, msg.as_ptr());
			unreachable!()
		}
	}

	pub unsafe fn push_local_frame(&self, capacity: isize) -> bool {
		((**self.ptr).PushLocalFrame)(self.ptr, capacity as jint) == JniError::JNI_OK
	}

	pub unsafe fn pop_local_frame<T: JObject<'a>>(&self, result: &'a T) -> T {
		T::from_jobject(self.clone(), ((**self.ptr).PopLocalFrame)(self.ptr, result.get_obj()))
	}

	pub fn is_same_object<T1: JObject<'a>, T2: JObject<'a>>(&self, obj1: &T1, obj2: &T2, _cap: &Capability) -> bool {
		unsafe {
			((**self.ptr).IsSameObject)(self.ptr, obj1.get_obj(), obj2.get_obj()) != 0
		}
	}

	pub fn is_null<T: 'a + JObject<'a>>(&self, obj1: &T, _cap: &Capability) -> bool {
		unsafe {
			((**self.ptr).IsSameObject)(self.ptr, obj1.get_obj(), 0 as jobject) != 0
		}
	}

	unsafe fn new_local_ref<T: 'a + JObject<'a>>(&self, lobj: &T) -> jobject {
		((**self.ptr).NewLocalRef)(self.ptr, lobj.get_obj())
	}

	fn delete_local_ref<T: 'a + JObject<'a>>(&self, gobj: T) {
		unsafe {
			((**self.ptr).DeleteLocalRef)(self.ptr, gobj.get_obj())
		}
	}

	unsafe fn new_global_ref<T: 'a + JObject<'a>>(&self, lobj: &T) -> jobject {
		((**self.ptr).NewGlobalRef)(self.ptr, lobj.get_obj())
	}

	fn delete_global_ref<T: 'a + JObject<'a>>(&self, gobj: T) {
		unsafe {
			((**self.ptr).DeleteGlobalRef)(self.ptr, gobj.get_obj())
		}
	}

	unsafe fn new_weak_ref<T: 'a + JObject<'a>>(&self, lobj: &T) -> jweak {
		((**self.ptr).NewWeakGlobalRef)(self.ptr, lobj.get_obj())
	}

	fn delete_weak_ref<T: 'a + JObject<'a>>(&self, wobj: T) {
		unsafe {
			((**self.ptr).DeleteWeakGlobalRef)(self.ptr, wobj.get_obj() as jweak)
		}
	}

	pub fn ensure_local_capacity(&self, capacity: isize, cap: Capability) -> Result<Capability, Exception> {
		if unsafe {
			((**self.ptr).EnsureLocalCapacity)(self.ptr, capacity as jint) == JniError::JNI_OK
		} {
			Ok(cap)
		} else {
			Err(Exception::new())
		}
	}

	pub fn alloc_object(&self, clazz: &JavaClass, cap: Capability) -> ThisResult<JavaObject> {
		unsafe {
			JObject::from_unless_null(self.clone(),
									  ((**self.ptr).AllocObject)(self.ptr, clazz.ptr),
									  cap)
		}
	}

	pub fn monitor_enter<T: 'a + JObject<'a>>(&self, obj: &T) -> bool {
		unsafe {
			((**self.ptr).MonitorEnter)(self.ptr, obj.get_obj()) == JniError::JNI_OK
		}
	}

	pub fn monitor_exit<T: 'a + JObject<'a>>(&self, obj: &T) -> bool {
		unsafe {
			((**self.ptr).MonitorExit)(self.ptr, obj.get_obj()) == JniError::JNI_OK
		}
	}

	// pub fn jvm(&self) -> &mut JavaVM {
	//     JavaVM::from(unsafe {
	//         let mut jvm: *mut JavaVMImpl = 0 as *mut JavaVMImpl;
	//         ((**self.ptr).GetJavaVM)(self.ptr, &mut jvm);
	//         jvm
	//     })
	// }

	pub fn exception_check(&self) -> bool {
		unsafe {
			((**self.ptr).ExceptionCheck)(self.ptr) != 0
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefType {
	Local,
	Global,
	Weak,
}

pub type ThisResult<T> = Result<(T, Capability), Exception>;

pub trait JObject<'a>: Drop {
	fn get_env(&self) -> JavaEnv<'a>;
	fn get_obj(&self) -> jobject;
	fn ref_type(&self) -> RefType;
	unsafe fn from_parts_type(env: JavaEnv<'a>, ptr: jobject, typ: RefType, cap: Capability) -> ThisResult<Self>;
	unsafe fn from_parts(env: JavaEnv<'a>, ptr: jobject, cap: Capability) -> ThisResult<Self>;
	unsafe fn from_jobject(env: JavaEnv<'a>, ptr: jobject) -> Self;
	unsafe fn from_unless_null(env: JavaEnv<'a>, ptr: jobject, cap: Capability) -> ThisResult<Self>;
	fn global(&'a self, cap: Capability) -> ThisResult<Self>;
	fn weak(&'a self, cap: Capability) -> ThisResult<Self>;

	fn inc_ref(&self) -> jobject {
		let env = self.get_env();
		match self.ref_type() {
			RefType::Local => unsafe {
				((**env.ptr).NewLocalRef)(env.ptr, self.get_obj())
			},
			RefType::Global => unsafe {
				((**env.ptr).NewGlobalRef)(env.ptr, self.get_obj())
			},
			RefType::Weak => unsafe {
				((**env.ptr).NewWeakGlobalRef)(env.ptr, self.get_obj()) as jobject
			},
		}
	}

	fn dec_ref(&mut self) {
		let env = self.get_env();
		match self.ref_type() {
			RefType::Local => unsafe {
				((**env.ptr).DeleteLocalRef)(env.ptr, self.get_obj())
			},
			RefType::Global => unsafe {
				((**env.ptr).DeleteGlobalRef)(env.ptr, self.get_obj())
			},
			RefType::Weak => unsafe {
				((**env.ptr).DeleteWeakGlobalRef)(env.ptr, self.get_obj())
			},
		}
	}

	fn get_class(&'a self, cap: Capability) -> ThisResult<JavaClass<'a>> {
		let env = self.get_env();
		unsafe {
			JObject::from_parts(env.clone(),
								((**env.ptr).GetObjectClass)(env.ptr, self.get_obj()) as jobject
								, cap)
		}
	}

	fn as_jobject(&'a self) -> JavaObject {
		JavaObject{
			env: self.get_env(),
			ptr: self.inc_ref(),
			rtype: self.ref_type()
		}
	}

	fn is_instance_of(&self, clazz: &JavaClass, _cap: &Capability) -> bool {
		let env = self.get_env();
		unsafe {
			((**env.ptr).IsInstanceOf)(env.ptr, self.get_obj(), clazz.ptr) != 0
		}
	}

	fn is_same<'b, T: 'b + JObject<'b>>(&self, val: &T) -> bool {
		let env = self.get_env();
		unsafe {
			((**env.ptr).IsSameObject)(env.ptr, self.get_obj(), val.get_obj()) != 0
		}

	}

	fn is_null(&self) -> bool {
		let val = self.get_env();
		unsafe {
			((**val.ptr).IsSameObject)(val.ptr, self.get_obj(), 0 as jobject) != 0
		}
	}
}
// pub trait JArray<'a, T: 'a + JObject<'a>>: JObject<'a> {}


macro_rules! impl_jobject(
	($cls:ident, $native:ident) => (
		impl<'a> Drop for $cls<'a> {
			fn drop(&mut self) {
				self.dec_ref();
			}
		}

		impl<'a> JObject<'a> for $cls<'a> {
			fn get_env(&self) -> JavaEnv<'a> {
				self.env.clone()
			}

			fn get_obj(&self) -> jobject {
				self.ptr as jobject
			}

			fn ref_type(&self) -> RefType {
				self.rtype
			}

			unsafe fn from_jobject(env: JavaEnv<'a>, ptr: jobject) -> $cls<'a> {
				return $cls {
					env: env.clone(),
					ptr: ptr as $native,
					rtype: RefType::Local,
				}
			}

			unsafe fn from_parts(env: JavaEnv<'a>, ptr: jobject, cap: Capability) -> ThisResult<$cls> {
				$cls::from_parts_type(env, ptr, RefType::Local, cap)
			}

			unsafe fn from_parts_type(env: JavaEnv<'a>, ptr: jobject, typ: RefType, cap: Capability) -> ThisResult<$cls<'a>> {
				if env.exception_check() {
					Err(Exception::new())
				} else {
					Ok(($cls {
						env: env.clone(),
						ptr: ptr as $native,
						rtype: typ,
					}, cap))
				}
			}

			unsafe fn from_unless_null(env: JavaEnv<'a>, ptr: jobject, cap: Capability) -> ThisResult<$cls<'a>> {
				if ptr as usize == 0 {
					Err(Exception::new())
				} else {
					Ok(($cls {
						env: env.clone(),
						ptr: ptr as $native,
						rtype: RefType::Local,
					}, cap))
				}
			}


			fn global(&self, cap: Capability) -> ThisResult<$cls<'a>> {
				let env = self.get_env();
				unsafe {
					$cls::from_parts_type(env.clone(), env.new_global_ref(self), RefType::Global, cap)
				}
			}

			fn weak(&self, cap: Capability) -> ThisResult<$cls<'a>> {
				let env = self.get_env();
				unsafe {
					$cls::from_parts_type(env.clone(), env.new_weak_ref(self), RefType::Weak, cap)
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
	env: JavaEnv<'a>,
	ptr: jobject,
	rtype: RefType,
}



impl_jobject!(JavaObject, jobject);


#[derive(Debug)]
pub struct JavaClass<'a> {
	env: JavaEnv<'a>,
	ptr: jclass,
	rtype: RefType,
}

impl_jobject!(JavaClass, jclass);

impl<'a> JavaClass<'a> {
	pub fn get_super(&self, _cap: &Capability) -> Option<JavaClass<'a>> {
		if self.ptr.is_null() {
			return None
		}
		let env = self.get_env();
		let ptr = unsafe {
			((**env.ptr).GetSuperclass)(env.ptr, self.ptr) as jobject
		};
		if ptr.is_null() {
			None
		} else {
			Some(JavaClass {
				env: env.clone(),
				ptr: ptr as jclass,
				rtype: RefType::Local,
			})
		}
	}

	pub fn alloc(&self, cap: Capability) -> ThisResult<JavaObject> {
		let env = self.get_env();
		unsafe {
			let ptr: jobject = ((**env.ptr).AllocObject)(env.ptr, self.ptr);
			JObject::from_unless_null(env, ptr, cap)
		}
	}

	pub fn find(env: &'a JavaEnv, name: &JavaChars, cap: Capability) -> ThisResult<JavaClass<'a>> {
		env.find_class(name, cap)
	}
}


#[derive(Debug)]
pub struct JavaThrowable<'a> {
	env: JavaEnv<'a>,
	ptr: jthrowable,
	rtype: RefType,
}

impl_jobject!(JavaThrowable, jthrowable);

#[derive(Debug)]
pub struct JavaString<'a> {
	env: JavaEnv<'a>,
	ptr: jstring,
	rtype: RefType,
}

impl_jobject!(JavaString, jstring);

use super::j_chars::JavaChars;
impl<'a> JavaString<'a> {
	pub fn new(env: &'a JavaEnv<'a>, val: &super::j_chars::JavaChars, cap: Capability) -> ThisResult<JavaString<'a>> {
		unsafe {
			JObject::from_parts(env.clone(),
								((**env.ptr).NewStringUTF)(env.ptr, val.as_ptr()) as jobject,
								cap)
		}
	}

	pub fn len(&self) -> usize {
		unsafe {
			((**self.get_env().ptr).GetStringLength)(self.get_env().ptr, self.ptr) as usize
		}
	}

	pub fn size(&self) -> usize {
		unsafe {
			((**self.get_env().ptr).GetStringUTFLength)(self.get_env().ptr, self.ptr) as usize
		}
	}

	pub fn to_str(&self) -> Option<string::String> {
		let (chars, _) = self.chars();
		chars.to_str()
	}

	fn chars(&self) -> (JavaStringChars, bool) {
		let mut isCopy: jboolean = 0;
		let result = JavaStringChars{
			s: &self,
			chars: unsafe {
				((**self.get_env().ptr).GetStringUTFChars)(self.get_env().ptr,
														   self.ptr, &mut isCopy)
			}
		};
		(result, isCopy != 0)
	}

	pub fn region(&self, start: usize, length: usize) -> JavaChars {
		let mut vec: Vec<u8> = Vec::with_capacity(length + 1);
		unsafe {
			((**self.get_env().ptr).GetStringUTFRegion)(
				self.get_env().ptr, self.ptr, start as jsize,
				length as jsize, vec.as_mut_ptr() as *mut ::libc::c_char);
			vec.set_len(length + 1);
		}
		vec[length] = 0;
		unsafe {
			JavaChars::from_raw_vec(vec)
		}
	}
}

struct JavaStringChars<'a> {
	s: &'a JavaString<'a>,
	chars: *const ::libc::c_char
}

impl<'a> Drop for JavaStringChars<'a> {
	fn drop(&mut self) {
		unsafe {
			((**self.s.env.ptr).ReleaseStringUTFChars)(
				self.s.env.ptr, self.s.ptr, self.chars)
		}
	}
}


impl<'a> fmt::Debug for JavaStringChars<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "\"{:?}\"", self.to_str())
	}
}

impl<'a> JavaStringChars<'a> {
	fn to_str(&self) -> Option<string::String> {
		unsafe {
			super::j_chars::JavaChars::from_raw_vec(
				::std::ffi::CStr::from_ptr(self.chars).to_bytes_with_nul().to_vec()
					)
		}.to_string()
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
	env: JavaEnv<'a>,
	ptr: jarray,
	rtype: RefType,
	phantom: PhantomData<T>,
}

impl<'a, T: 'a + JObject<'a>> Drop for JavaArray<'a, T> {
	fn drop(&mut self) {
		let env = self.get_env();
		match self.ref_type() {
			RefType::Local => unsafe {
				((**env.ptr).DeleteLocalRef)(env.ptr, self.get_obj())
			},
			RefType::Global => unsafe {
				((**env.ptr).DeleteGlobalRef)(env.ptr, self.get_obj())
			},
			RefType::Weak => unsafe {
				((**env.ptr).DeleteWeakGlobalRef)(env.ptr, self.get_obj())
			},
		}
	}
}

// impl<'a, T: 'a + JObject<'a>> JavaArray<'a,T> {
//     fn dup(&self) -> JavaArray<T> {
//         JavaArray{
//             env: self.get_env(),
//             ptr: self.inc_ref(),
//             rtype: self.rtype,
//             phantom: PhantomData::<T>,
//         }
//     }
// }

impl<'a, T: 'a + JObject<'a>> JObject<'a> for JavaArray<'a, T> {
	fn get_env(&self) -> JavaEnv<'a> {
		self.env.clone()
	}

	fn get_obj(&self) -> jobject {
		self.ptr as jobject
	}

	fn ref_type(&self) -> RefType {
		self.rtype
	}

	unsafe fn from_jobject(env: JavaEnv<'a>, ptr: jobject) -> JavaArray<'a, T> {
		JavaArray{
			env: env.clone(),
			ptr: ptr as jarray,
			rtype: RefType::Local,
			phantom: PhantomData::<T>,
		}
	}

	unsafe fn from_parts_type(env: JavaEnv<'a>, ptr: jobject, typ: RefType, cap: Capability) -> ThisResult<JavaArray<T>> {
		if env.exception_check() {
			Err(Exception::new())
		} else {
			Ok((JavaArray{
				env: env.clone(),
				ptr: ptr as jarray,
				rtype: typ,
				phantom: PhantomData::<T>,
			}, cap))
		}
	}

	unsafe fn from_parts(env: JavaEnv<'a>, ptr: jobject, cap: Capability) -> ThisResult<JavaArray<T>> {
		JavaArray::from_parts_type(env, ptr, RefType::Local, cap)
	}

	unsafe fn from_unless_null(env: JavaEnv<'a>, ptr: jobject, cap: Capability) -> ThisResult<JavaArray<T>> {
		if ptr as usize == 0 {
			Err(Exception::new())
		} else {
			Ok((JavaArray{
				env: env.clone(),
				ptr: ptr as jarray,
				rtype: RefType::Local,
				phantom: PhantomData::<T>,
			}, cap))
		}
	}

	fn global(&'a self, cap: Capability) -> ThisResult<JavaArray<T>> {
		unsafe { JavaArray::from_parts(self.env.clone(), self.env.new_global_ref(self), cap) }
	}

	fn weak(&'a self, cap: Capability) -> ThisResult<JavaArray<T>> {
		unsafe { JavaArray::from_parts(self.env.clone(), self.get_env().new_weak_ref(self), cap) }
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
			JniVersion::JNI_VERSION_1_6,
			&[JavaVMOption::new("-Xcheck:jni"), JavaVMOption::new("-ea")],
			false
		);
		assert!(!args.ignoreUnrecognized);
		assert!(args.version == JniVersion::JNI_VERSION_1_6);
		assert!(args.options.len() == 2);
		assert!(args.options[0] == "-Xcheck:jni");
		assert!(args.options == ["-Xcheck:jni", "-ea"]);
	}

	#[test]
	fn test_JavaVMAttachArgs() {
	}

	#[test]
	fn test_JavaEnv() {
		let (mut jvm, cap) = JavaVM::new(
			JavaVMInitArgs::new(
				JniVersion::JNI_VERSION_1_6,
				&[JavaVMOption::new("-Xcheck:jni")/*, JavaVMOption::new("-verbose:jni")*/],
				false,
			),
		"1").unwrap();
		assert!(jvm.version() == JniVersion::JNI_VERSION_1_6);

		let ver = jvm.version();
		let t = jvm.get_env();
		assert!(!t.is_err());

		let env = t.unwrap();
		assert!(env.version(&cap) >= ver);
	}
}
