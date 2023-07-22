use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{fence, AtomicUsize, Ordering};

// チケットロック用の型
pub struct TicketLock<T> {
    ticket: AtomicUsize, // チケット
    turn: AtomicUsize,   // 実行可能なチケット
    pub data: UnsafeCell<T>,
}

// ロック解放と､保護対象データへのアクセスを行うための型
pub struct TicketLockGuard<'a, T> {
    ticket_lock: &'a TicketLock<T>,
}

impl<T> TicketLock<T> {
    pub fn new(v: T) -> Self {
        TicketLock {
            ticket: AtomicUsize::new(0),
            turn: AtomicUsize::new(0),
            data: UnsafeCell::new(v),
        }
    }

    // ロック用関数
    pub fn lock(&self) -> TicketLockGuard<T> {
        // チケットを取得
        let t = self.ticket.fetch_add(1, Ordering::Release);
        // 所有するチケットの番号になるまでスピン
        while self.turn.load(Ordering::Relaxed) != t {}
        fence(Ordering::Release);

        TicketLockGuard { ticket_lock: self }
    }
}

// ロック獲得後自動で解放されるようにDropトレイトを実装
impl<'a, T> Drop for TicketLockGuard<'a, T> {
    fn drop(&mut self) {
        self.ticket_lock.turn.fetch_add(1, Ordering::Relaxed);
    }
}

impl<'a, T> Deref for TicketLockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ticket_lock.data.get() }
    }
}

impl<'a, T> DerefMut for TicketLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.ticket_lock.data.get() }
    }
}
