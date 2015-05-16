use ::std::mem;
use ::std::fmt;
use ::std::string;
use ::std::ffi::CString;

use super::native::*;


/// Stores an option for the JVM
#[allow(raw_pointer_derive)]
#[derive(Debug, Clone)]
#[repr(C)]
pub struct JavaVMOption {
	/// The option to be passed to the JVM
	pub optionString: string::String,

	/// Extra info for the JVM. This interface always sets it to `null`.
	pub extraInfo: *const ::libc::c_void
}

impl JavaVMOption {
	/// Constructs a new `JavaVMOption`
	pub fn new(option: &str, extra: *const ::libc::c_void) -> JavaVMOption {
		JavaVMOption{
			optionString: option.to_string(),
			extraInfo: extra
		}
	}
}

/// Stores a vector of options to be passed to the JVM at JVM startup
#[allow(raw_pointer_derive)]
#[derive(Debug)]
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
	pub name: string::String,
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
	name: Box<String>,
}

impl JavaVM {
	/// Creates a Java Virtual Machine.
	/// The JVM will automatically be destroyed when this class goes out of scope.
	pub fn new(args: JavaVMInitArgs, name: &str) -> Result<JavaVM,JniError> {
		use ::std::borrow::ToOwned;
		let (res, jvm) = unsafe {
			let mut jvm: *mut JavaVMImpl = 0 as *mut JavaVMImpl;
			let mut env: *mut JNIEnvImpl = 0 as *mut JNIEnvImpl;
			let mut vm_opts = vec![];
			let mut vm_opts_vect = vec![];
			for opt in args.options.iter() {
				let cstr:CString = CString::new(&opt.optionString[..]).unwrap();
				vm_opts.push(
					JavaVMOptionImpl {
						optionString: cstr.as_ptr(),
						extraInfo: opt.extraInfo
					});
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
			JniError::JNI_OK => Ok(JavaVM{
				ptr: jvm,
				version: args.version,
				name: Box::new(name[..].to_owned())
			}),
			_ => Err(res)
		}
	}
/*
	pub fn from(ptr: *mut JavaVMImpl) -> JavaVM {
		let mut res = JavaVM{
			ptr: ptr,
			version: JniVersion::JNI_VERSION_1_1,
			name: Box::<String>::new(String.new()),
		};
		res.version = res.get_env().version();
		res
	}
*/
	fn ptr(&self) -> *mut JavaVMImpl {
		self.ptr
	}

	pub fn version(&self) -> JniVersion {
		return self.version
	}

	pub fn get_env(&mut self) -> JavaEnv {
		unsafe {
			let ref jni = **self.ptr;
			self.get_env_gen(jni.AttachCurrentThread)
		}
	}

	pub fn get_env_daemon(&mut self) -> JavaEnv {
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

	unsafe fn get_env_gen(&mut self, fun: extern "C" fn(vm: *mut JavaVMImpl, penv: &mut *mut JNIEnvImpl, args: *mut JavaVMAttachArgsImpl) -> JniError) -> JavaEnv {
		let mut env: *mut JNIEnvImpl = 0 as *mut JNIEnvImpl;
		let res = ((**self.ptr).GetEnv)(self.ptr, &mut env, self.version());
		match res {
			JniError::JNI_OK => JavaEnv { ptr: &mut *env, phantom: PhantomData, },
			JniError::JNI_EDETACHED => {
				let mut attachArgs = JavaVMAttachArgsImpl{
					version: self.version(),
					name: self.name.as_ptr() as *const ::libc::c_char,
					group: 0 as jobject
				};
				let res = fun(self.ptr, &mut env, &mut attachArgs);
				match res {
					JniError::JNI_OK => JavaEnv { ptr: &mut *env, phantom: PhantomData, },
					_ => panic!("AttachCurrentThread error {:?}!", res)
				}
			},
			JniError::JNI_EVERSION => panic!("Version {:?} is not supported by GetEnv!", self.version()),
			_ => panic!("GetEnv error {:?}!", res)
		}
	}

	unsafe fn destroy_java_vm(&self) -> bool {
		((**self.ptr).DestroyJavaVM)(self.ptr) == JniError::JNI_OK
	}
}

impl Drop for JavaVM {
	fn drop(&mut self) {
		unsafe {
			self.destroy_java_vm();
		}
	}
}

/// Represents an environment pointer used by the JNI.
/// Serves as an upper bound to the lifetime of all local refs
/// created by this binding.
///
/// TODO: allow for global/weak refs to outlive their env.
#[derive(Debug, Clone)]
pub struct JavaEnv<'a> {
	ptr: *mut JNIEnvImpl,
	phantom: PhantomData<&'a JavaVM>,
}

impl<'a> JavaEnv<'a> {
	pub fn version(&self) -> JniVersion {
		unsafe {
			mem::transmute(((**self.ptr).GetVersion)(self.ptr))
		}
	}

	pub fn ptr(&self) -> *mut JNIEnvImpl {
		self.ptr
	}

	pub fn define_class<'b, T: 'b + JObject<'b>>(&self, name: &JavaChars, loader: &T, buf: &[u8], len: usize) -> JavaClass {
		JObject::from(
			self.clone(),
			unsafe { ((**self.ptr).DefineClass)(
				self.ptr,
				name.as_ptr() as *const ::libc::c_char,
				loader.get_obj(),
				buf.as_ptr() as *const jbyte,
				len as jsize
			) } as jobject
		)
	}

	// Takes a string and returns a Java class if successfull.
	// Returns `None` on failure.
	pub fn find_class(&self, name: &JavaChars) -> Option<JavaClass> {
		let ptr = unsafe { ((**self.ptr).FindClass)(
			self.ptr, name.as_ptr()) };
		if ptr == (0 as jclass) {
			None
		} else {
			Some(JObject::from(self.clone(), ptr as jobject))
		}
	}

	pub fn get_super_class(&self, sub: &JavaClass) -> JavaClass {
		JObject::from(self.clone(), unsafe {
			((**self.ptr).GetSuperclass)(self.ptr, sub.ptr) as jobject
		})
	}

	pub fn is_assignable_from(&self, sub: &JavaClass, sup: &JavaClass) -> bool {
		unsafe {
			((**self.ptr).IsAssignableFrom)(self.ptr, sub.ptr, sup.ptr) != 0
		}
	}


	pub fn throw(&self, obj: &JavaThrowable) -> bool {
		unsafe {
			((**self.ptr).Throw)(self.ptr, obj.ptr) == JniError::JNI_OK
		}
	}

	pub fn throw_new(&self, clazz: &JavaClass, msg: &JavaChars) -> bool {
		unsafe {
			((**self.ptr).ThrowNew)(self.ptr, clazz.ptr, msg.as_ptr() as *const ::libc::c_char) == JniError::JNI_OK
		}
	}

	pub fn exception_occured(&self) -> JavaThrowable {
		JObject::from(
			self.clone(),
			unsafe {
				((**self.ptr).ExceptionOccurred)(self.ptr) as jobject
			}
		)
	}

	pub fn exception_describe(&self) {
		unsafe {
			((**self.ptr).ExceptionDescribe)(self.ptr)
		}
	}

	pub fn exception_clear(&self) {
		unsafe {
			((**self.ptr).ExceptionClear)(self.ptr)
		}
	}

	pub fn fatal_error(&self, msg: &JavaChars) {
		unsafe {
			((**self.ptr).FatalError)(self.ptr, msg.as_ptr())
		}
	}

	pub fn push_local_frame(&self, capacity: isize) -> bool {
		unsafe {
			((**self.ptr).PushLocalFrame)(self.ptr, capacity as jint) == JniError::JNI_OK
		}
	}

	pub fn pop_local_frame<T: JObject<'a>>(&self, result: &'a T) -> T {
		JObject::from(self.clone(), unsafe {
			((**self.ptr).PopLocalFrame)(self.ptr, result.get_obj())
		})
	}

	pub fn is_same_object<T1: JObject<'a>, T2: JObject<'a>>(&self, obj1: &T1, obj2: &T2) -> bool {
		unsafe {
			((**self.ptr).IsSameObject)(self.ptr, obj1.get_obj(), obj2.get_obj()) != 0
		}
	}

	pub fn is_null<T: 'a + JObject<'a>>(&self, obj1: &T) -> bool {
		unsafe {
			((**self.ptr).IsSameObject)(self.ptr, obj1.get_obj(), 0 as jobject) != 0
		}
	}

	fn new_local_ref<T: 'a + JObject<'a>>(&self, lobj: &T) -> jobject {
		unsafe {
			((**self.ptr).NewLocalRef)(self.ptr, lobj.get_obj())
		}
	}

	fn delete_local_ref<T: 'a + JObject<'a>>(&self, gobj: T) {
		unsafe {
			((**self.ptr).DeleteLocalRef)(self.ptr, gobj.get_obj())
		}
	}

	fn new_global_ref<T: 'a + JObject<'a>>(&self, lobj: &T) -> jobject {
		unsafe {
			((**self.ptr).NewGlobalRef)(self.ptr, lobj.get_obj())
		}
	}

	fn delete_global_ref<T: 'a + JObject<'a>>(&self, gobj: T) {
		unsafe {
			((**self.ptr).DeleteGlobalRef)(self.ptr, gobj.get_obj())
		}
	}

	fn new_weak_ref<T: 'a + JObject<'a>>(&self, lobj: &T) -> jweak {
		unsafe {
			((**self.ptr).NewWeakGlobalRef)(self.ptr, lobj.get_obj())
		}
	}

	fn delete_weak_ref<T: 'a + JObject<'a>>(&self, wobj: T) {
		unsafe {
			((**self.ptr).DeleteWeakGlobalRef)(self.ptr, wobj.get_obj() as jweak)
		}
	}

	pub fn ensure_local_capacity(&self, capacity: isize) -> bool {
		unsafe {
			((**self.ptr).EnsureLocalCapacity)(self.ptr, capacity as jint) == JniError::JNI_OK
		}
	}

	pub fn alloc_object(&self, clazz: &JavaClass) -> JavaObject {
		JObject::from(self.clone(), unsafe {
			((**self.ptr).AllocObject)(self.ptr, clazz.ptr)
		})
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
/*
	pub fn jvm(&self) -> &mut JavaVM {
		JavaVM::from(unsafe {
			let mut jvm: *mut JavaVMImpl = 0 as *mut JavaVMImpl;
			((**self.ptr).GetJavaVM)(self.ptr, &mut jvm);
			jvm
		})
	}
*/
	pub fn exception_check(&self) -> bool {
		unsafe {
			((**self.ptr).ExceptionCheck)(self.ptr) != 0
		}
	}
}

#[derive(Debug, Clone, Copy)]
enum RefType {
	Local,
	Global,
	Weak,
}

pub trait JObject<'a>: Drop {
	fn get_env(&self) -> JavaEnv<'a>;
	fn get_obj(&self) -> jobject;
	fn ref_type(&self) -> RefType;

	fn from(env: JavaEnv<'a>, ptr: jobject) -> Self;
	fn global(&'a self) -> Self;
	fn weak(&'a self) -> Self;

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

	fn get_class(&'a self) -> JavaClass<'a> {
		let env = self.get_env();
		JObject::from(env.clone(), unsafe {
			((**env.ptr).GetObjectClass)(env.ptr, self.get_obj()) as jobject
		})
	}

	fn as_jobject(&'a self) -> JavaObject {
		JavaObject{
			env: self.get_env(),
			ptr: self.inc_ref(),
			rtype: self.ref_type()
		}
	}

	fn is_instance_of(&self, clazz: &JavaClass) -> bool {
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
/*
pub trait JArray<'a, T: 'a + JObject<'a>>: JObject<'a> {
}
*/

macro_rules! impl_jobject(
	($cls:ident, $native:ident) => (
		impl<'a> Drop for $cls<'a> {
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
/*
		impl<'a> $cls<'a> {
			fn copy(&self) -> $cls {
				$cls {
					env: self.get_env(),
					ptr: self.inc_ref(),
					rtype: self.rtype
				}
			}
		}
*/
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

			fn from(env: JavaEnv<'a>, ptr: jobject) -> $cls<'a> {
				$cls{
					env: env.clone(),
					ptr: ptr as $native,
					rtype: RefType::Local,
				}
			}

			fn global(&self) -> $cls<'a> {
				let env = self.get_env();
				$cls{
					env: env.clone(),
					ptr: env.new_global_ref(self),
					rtype: RefType::Global
				}
			}

			fn weak(&self) -> $cls<'a> {
				let env = self.get_env();
				$cls {
					env: env.clone(),
					ptr: env.new_weak_ref(self),
					rtype: RefType::Weak
				}
			}
		}
	);
);

macro_rules! impl_jarray(
	($cls:ident, $native:ident) => (
		impl_jobject!($cls, $native);

		// impl $cls {
		//		pub fn as_jarray(&self) -> JavaArray {
		//			self.inc_ref();
		//			JavaArray {
		//				env: self.get_env(),
		//				ptr: self.ptr as jarray
		//			}
		//		}
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
	pub fn get_super(&'a self) -> JavaClass<'a> {
		let env = self.get_env();
		JObject::from(env.clone(), unsafe {
			((**env.ptr).GetSuperclass)(env.ptr, self.ptr) as jobject
		})
	}

	pub fn alloc(&self) -> JavaObject {
		let env = self.get_env();
		JObject::from(env.clone(), unsafe {
			((**env.ptr).AllocObject)(env.ptr, self.ptr)
		})
	}

	pub fn find(env: &'a JavaEnv, name: &JavaChars) -> Option<JavaClass<'a>> {
		env.find_class(name)
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
	pub fn new(env: &'a JavaEnv<'a>, val: &super::j_chars::JavaChars) -> JavaString<'a> {
		JObject::from(env.clone(), unsafe {
			((**env.ptr).NewStringUTF)(env.ptr, val.as_ptr()) as jobject
		})
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
			((**self.get_env().ptr).GetStringUTFRegion)(self.get_env().ptr, self.ptr, start as jsize, length as jsize, vec.as_mut_ptr() as *mut ::libc::c_char);
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
			((**self.s.env.ptr).ReleaseStringUTFChars)(self.s.env.ptr, self.s.ptr,
													   self.chars)
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

/*
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
*/

use ::std::marker::PhantomData;
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
/*
impl<'a, T: 'a + JObject<'a>> JavaArray<'a,T> {
	fn dup(&self) -> JavaArray<T> {
		JavaArray{
			env: self.get_env(),
			ptr: self.inc_ref(),
			rtype: self.rtype,
			phantom: PhantomData::<T>,
		}
	}
}
*/
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

	fn from(env: JavaEnv<'a>, ptr: jobject) -> JavaArray<T> {
		JavaArray{
			env: env.clone(),
			ptr: ptr as jarray,
			rtype: RefType::Local,
			phantom: PhantomData::<T>,
		}
	}

	fn global(&self) -> JavaArray<T> {
		let env = self.get_env();
		JavaArray{
			env: env.clone(),
			ptr: env.new_global_ref(self),
			rtype: RefType::Global,
			phantom: PhantomData::<T>,
		}
	}

	fn weak(&self) -> JavaArray<T> {
		let env = self.get_env();
		JavaArray{
			env: env.clone(),
			ptr: env.new_weak_ref(self),
			rtype: RefType::Weak,
			phantom: PhantomData::<T>,
		}
	}
}
/*

unsafe fn JavaVMOptionImpl_new(opt: &::jni::JavaVMOption) -> JavaVMOptionImpl {
	let cstring = CString::unchecked_from_bytes(opt.optionString[..].as_bytes());
	JavaVMOptionImpl{
		optionString: cstring.as_ptr(),// opt.optionString[..].as_ptr() as * const ::libc::c_char, // TOSO: remove odd cast
		extraInfo: opt.extraInfo
	}
}

*/
