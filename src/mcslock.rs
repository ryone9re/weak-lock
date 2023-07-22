use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::ptr::null_mut;
use std::sync::atomic::{fence, AtomicBool, AtomicPtr, Ordering};

pub struct MCSLock<T> {
    last: AtomicPtr<MCSNode<T>>, // キューの最後尾
    data: UnsafeCell<T>,         // 保護対象データ
}

pub struct MCSNode<T> {
    next: AtomicPtr<MCSNode<T>>, // 次のノード
    locked: AtomicBool,          // trueならロック獲得中
}

pub struct MCSLockGuard<'a, T> {
    node: &'a mut MCSNode<T>, // 自スレッドのノード
    mcs_lock: &'a MCSLock<T>, // キューの最後尾と保護対象データへの参照
}

// スレッド間のデータ共有と､チャネルを使った送受信が可能と設定
unsafe impl<T> Sync for MCSLock<T> {}
unsafe impl<T> Send for MCSLock<T> {}

impl<T> MCSNode<T> {
    pub fn new() -> Self {
        MCSNode {
            next: AtomicPtr::new(null_mut()),
            locked: AtomicBool::new(false),
        }
    }
}

impl<T> Default for MCSNode<T> {
    fn default() -> Self {
        Self::new()
    }
}

// 保護対象データのimutableな参照外し
impl<'a, T> Deref for MCSLockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mcs_lock.data.get() }
    }
}
// 保護対象データのmutableな参照外し
impl<'a, T> DerefMut for MCSLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mcs_lock.data.get() }
    }
}

impl<T> MCSLock<T> {
    pub fn new(v: T) -> Self {
        MCSLock {
            last: AtomicPtr::new(null_mut()),
            data: UnsafeCell::new(v),
        }
    }

    pub fn lock<'a>(&'a self, node: &'a mut MCSNode<T>) -> MCSLockGuard<T> {
        // 自スレッド用のノードを初期化
        node.next = AtomicPtr::new(null_mut());
        node.locked = AtomicBool::new(false);

        let guard = MCSLockGuard {
            node,
            mcs_lock: self,
        };

        // 自身をキューの最後尾とする
        let ptr = guard.node as *mut MCSNode<T>;
        let prev = self.last.swap(ptr, Ordering::Relaxed);

        // 最後尾がNULLの場合は誰もロック獲得をしようとしていないためロック獲得
        // NULL以外の場合は､自身をキューの最後尾に追加
        if !prev.is_null() {
            // ロック獲得中と設定
            guard.node.locked.store(true, Ordering::Relaxed);

            // 自身をキューの最後尾に追加
            let prev = unsafe { &*prev };
            prev.next.store(ptr, Ordering::Relaxed);

            // 他のスレッドからfalseに設定されるまでスピン
            while guard.node.locked.load(Ordering::Relaxed) {
                std::hint::spin_loop()
            }
        }

        fence(Ordering::Acquire);

        guard
    }
}

impl<'a, T> Drop for MCSLockGuard<'a, T> {
    fn drop(&mut self) {
        // 自身の次のノードがNULLかつ自身が最後尾のノードなら､最後尾をNULLに設定
        if self.node.next.load(Ordering::Relaxed).is_null() {
            let ptr = self.node as *mut MCSNode<T>;
            if self
                .mcs_lock
                .last
                .compare_exchange(ptr, null_mut(), Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
        }

        // 自身の次のスレッドがlock関数実行中なので､その終了を待機
        while self.node.next.load(Ordering::Relaxed).is_null() {}

        // 自身の次のスレッドを実行可能に設定
        let next = unsafe { &mut *self.node.next.load(Ordering::Relaxed) };
        next.locked.store(false, Ordering::Relaxed);
    }
}
