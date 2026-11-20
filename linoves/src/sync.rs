//! Блокировка, безопасная относительно прерываний.
//!
//! Пока блокировка удерживается, прерывания на процессоре выключены,
//! поэтому обработчик прерывания никогда не встретит её занятой основным
//! кодом. Система однопроцессорная, так что если блокировка уже занята в
//! момент захвата — это гарантированно повторный вход (например, исключение
//! внутри критической секции). Вместо вечного зависания ядро сообщает об этом
//! понятной ошибкой.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

use crate::cpu;

pub struct IrqMutex<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for IrqMutex<T> {}
unsafe impl<T: Send> Send for IrqMutex<T> {}

pub struct IrqMutexGuard<'a, T> {
    mutex: &'a IrqMutex<T>,
    irq_was_enabled: bool,
}

impl<T> IrqMutex<T> {
    pub const fn new(value: T) -> Self {
        IrqMutex { locked: AtomicBool::new(false), data: UnsafeCell::new(value) }
    }

    #[track_caller]
    pub fn lock(&self) -> IrqMutexGuard<'_, T> {
        match self.try_lock() {
            Some(guard) => guard,
            None => panic!("deadlock prevented: lock is already held (re-entrant access)"),
        }
    }

    pub fn try_lock(&self) -> Option<IrqMutexGuard<'_, T>> {
        let irq_was_enabled = cpu::interrupts_enabled();
        cpu::disable_interrupts();
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(IrqMutexGuard { mutex: self, irq_was_enabled })
        } else {
            if irq_was_enabled {
                unsafe { cpu::enable_interrupts() };
            }
            None
        }
    }

    #[allow(dead_code)]
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }

    /// Принудительно снять блокировку. Используется только в аварийном
    /// пути (паника / фатальное исключение), когда система уже остановлена.
    pub unsafe fn force_unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}

impl<T> Deref for IrqMutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T> DerefMut for IrqMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T> Drop for IrqMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.locked.store(false, Ordering::Release);
        if self.irq_was_enabled {
            unsafe { cpu::enable_interrupts() };
        }
    }
}
