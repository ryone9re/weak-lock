use std::sync::Arc;

const NUM_LOOP: usize = 100000;
const NUM_THREADS: usize = 4;

use weak_lock::mcslock;

fn main() {
    let n = Arc::new(mcslock::MCSLock::new(0));
    let mut v = Vec::new();

    for _ in 0..NUM_THREADS {
        let n0 = n.clone();
        let t = std::thread::spawn(move || {
            // ノードを作成してロック
            let mut node = mcslock::MCSNode::new();
            for _ in 0..NUM_LOOP {
                let mut r = n0.lock(&mut node);
                *r += 1;
            }
        });

        v.push(t);
    }

    for t in v {
        t.join().unwrap();
    }

    // ノードを作成してロック
    let mut node = mcslock::MCSNode::new();
    let r = n.lock(&mut node);
    println!("COUNT = {} (expected = {})", *r, NUM_LOOP * NUM_THREADS);
}
