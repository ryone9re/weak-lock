use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{fence, AtomicBool, AtomicUsize, Ordering};

// スレッドの最大数
pub const NUM_LOCK: usize = 8;

// NUM_LOCKの剰余を求めるためのビットマスク
const MASK: usize = NUM_LOCK - 1;

// 公平なロック用の型
pub struct FairLock<T> {
    waiting: Vec<AtomicBool>, // ロック獲得試行中のスレッド
    lock: AtomicBool,         // ロック用変数
    turn: AtomicUsize,        // ロック獲得優先するスレッド
    data: UnsafeCell<T>,      // 保護対象データ
}

// ロックの解放と保護対象データへのアクセスを行うための型
pub struct FairLockGuard<'a, T> {
    fair_lock: &'a FairLock<T>,
    idx: usize, // スレッド番号
}

impl<T> FairLock<T> {
    pub fn new(v: T) -> Self {
        let mut vec = Vec::new();
        for _ in 0..NUM_LOCK {
            vec.push(AtomicBool::new(false));
        }

        FairLock {
            waiting: vec,
            lock: AtomicBool::new(false),
            turn: AtomicUsize::new(0),
            data: UnsafeCell::new(v),
        }
    }

    // ロック関数
    // idxはスレッドの番号
    pub fn lock(&self, idx: usize) -> FairLockGuard<T> {
        assert!(idx < NUM_LOCK); // idxが最大数未満であるか検査

        // 自身のスレッドをロック獲得試行中に設定
        self.waiting[idx].store(true, Ordering::Relaxed);

        loop {
            // 他のスレッドがfalseを設定した場合にロック獲得
            if !self.waiting[idx].load(Ordering::Relaxed) {
                break;
            }

            // 共有変数を用いてロック獲得を試みる
            if !self.lock.load(Ordering::Relaxed)
                && self
                    .lock
                    .compare_exchange_weak(
                        false,             // falseなら
                        true,              // trueを書き込み
                        Ordering::Relaxed, // 成功時のオーダー
                        Ordering::Relaxed, // 失敗時のオーダー
                    )
                    .is_ok()
            {
                break; // ロック獲得
            }
        }

        fence(Ordering::Acquire);

        FairLockGuard {
            fair_lock: self,
            idx,
        }
    }
}

impl<'a, T> Drop for FairLockGuard<'a, T> {
    fn drop(&mut self) {
        let fl = self.fair_lock; // fair_lockへの参照を取得

        // 自身のスレッドを非ロック獲得試行中に設定
        fl.waiting[self.idx].store(false, Ordering::Relaxed);

        // 現在のロック獲得優先スレッドが自分のスレッドなら次のスレッドに設定
        let turn = fl.turn.load(Ordering::Relaxed);
        let next = if turn == self.idx {
            (turn + 1) & MASK
        } else {
            turn
        };

        if fl.waiting[next].load(Ordering::Relaxed) {
            // 次のロック獲得優先スレッドがロック獲得中の場合
            // そのスレッドにロックを渡す
            fl.turn.store(next, Ordering::Relaxed);
            fl.waiting[next].store(false, Ordering::Relaxed);
        } else {
            // 次のロック獲得優先スレッドがロック獲得中でない場合
            // 次の次のスレッドをロック獲得優先スレッドに設定してロック解放
            fl.turn.store((next + 1) & MASK, Ordering::Relaxed);
            fl.lock.store(false, Ordering::Relaxed);
        }
    }
}

impl<'a, T> Deref for FairLockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.fair_lock.data.get() }
    }
}

impl<'a, T> DerefMut for FairLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.fair_lock.data.get() }
    }
}

unsafe impl<T> Sync for FairLock<T> {}
unsafe impl<T> Send for FairLock<T> {}
