use super::ffi;

#[repr(transparent)]
pub struct Context {
    base: ffi::sqlite3_context,
}

impl Context {
    pub fn as_ptr(&self) -> *const ffi::sqlite3_context {
        &self.base
    }

    pub fn set_result<T: ToContextResult>(&mut self, val: T) {
        val.assign_to(self);
    }
}

pub trait ToContextResult {
    fn assign_to(&self, context: &mut Context);
}

impl ToContextResult for i32 {
    fn assign_to(&self, context: &mut Context) {
        unsafe {
            ffi::result_int(context.as_ptr() as _, *self);
        }
    }
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Context").finish()
    }
}
