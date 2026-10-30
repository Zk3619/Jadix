#[cfg(jadix_naga_no_std)]
use alloc::boxed::Box;
#[cfg(jadix_naga_no_std)]
use once_cell::race::OnceBox;
#[cfg(jadix_naga_std)]
use std::sync::LazyLock;

#[cfg(jadix_naga_std)]
type Inner<T> = LazyLock<T, fn() -> T>;
#[cfg(jadix_naga_no_std)]
type Inner<T> = OnceBox<T>;

/// Lazy static helper that uses [`LazyLock`] with `std` and [`OnceBox`] otherwise.
///
/// [`LazyLock`]: https://doc.rust-lang.org/stable/std/sync/struct.LazyLock.html
/// [`OnceBox`]: https://docs.rs/once_cell/latest/once_cell/race/struct.OnceBox.html
#[derive(Debug)]
pub struct RacyLock<T: 'static> {
    inner: Inner<T>,
    #[cfg(jadix_naga_no_std)]
    init: fn() -> T,
}

impl<T: 'static> RacyLock<T> {
    #[cfg(jadix_naga_std)]
    /// Creates a new [`RacyLock`], which will initialize using the provided `init` function.
    pub const fn new(init: fn() -> T) -> Self {
        Self {
            inner: LazyLock::new(init),
        }
    }

    #[cfg(jadix_naga_no_std)]
    /// Creates a new [`RacyLock`], which will initialize using the provided `init` function.
    pub const fn new(init: fn() -> T) -> Self {
        Self {
            inner: OnceBox::new(),
            init,
        }
    }
}

#[cfg(jadix_naga_std)]
impl<T: 'static> core::ops::Deref for RacyLock<T> {
    type Target = T;

    /// Loads the internal value, initializing it if required.
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(jadix_naga_no_std)]
impl<T: 'static> core::ops::Deref for RacyLock<T> {
    type Target = T;

    /// Loads the internal value, initializing it if required.
    fn deref(&self) -> &Self::Target {
        self.inner.get_or_init(|| Box::new((self.init)()))
    }
}
