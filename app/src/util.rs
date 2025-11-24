use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
};

/// Prints the trimmed type name (e.g. with all paths removed). May not work correctly in all cases (likely breaks for local structs, futures, closures etc)
/// # Stability note
/// The output of this function is not guaranteed to be stable across rust versions (i.e. we forward the *lack* of stability guarantees inherent to [`std::any::type_name`])
/// As such, all usages of [`trimmed_type_name`] should be purely for programmer-facing debug output,
/// program behavior should not depend on the contents of the output.
#[must_use]
pub(crate) fn trimmed_type_name<T: ?Sized>() -> &'static str {
    let s = std::any::type_name::<T>();

    let mut last_ident_start_index = 0;
    let mut last_substr = "";
    let mut gen_flag = false;

    for substr in s.split("::") {
        last_substr = substr;
        if substr.contains('<') {
            gen_flag = true;
            break;
        }
        last_ident_start_index += substr.len() + 2;
    }
    if gen_flag {
        &s[last_ident_start_index..]
    } else {
        last_substr
    }
}
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoDebug<T> {
    inner: T,
}
impl<T> NoDebug<T> {
    pub(crate) fn into_inner(value: Self) -> T {
        value.inner
    }
}

impl<T> Debug for NoDebug<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = trimmed_type_name::<T>();
        f.debug_struct(&format!("{{ {n}, (no debug impl) }}"))
            .finish()
    }
}

impl<T> From<T> for NoDebug<T> {
    fn from(value: T) -> Self {
        Self { inner: value }
    }
}

impl<T> Deref for NoDebug<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for NoDebug<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
