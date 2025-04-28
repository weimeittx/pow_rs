use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::time::{Instant, Duration};
use sha2::{Sha256, Digest};
use std::sync::mpsc;
use std::thread;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// 固定字符串前缀
    #[arg(short, long, default_value = "weimeityy")]
    prefix: String,

    /// 需要的十六进制前导零数量
    #[arg(short, long, default_value_t = 6)]
    difficulty: usize,
}

fn main() {
    // 解析命令行参数
    let args = Args::parse();
    let prefix = args.prefix;
    let difficulty = args.difficulty;
    
    println!("开始POW挖矿，前缀: {}, 难度: {} 个十六进制前导零", prefix, difficulty);
    let start_time = Instant::now();
    
    // 用于发送找到的nonce
    let (sender, receiver) = mpsc::channel();
    // 用于通知其他线程停止工作
    let found = Arc::new(AtomicBool::new(false));
    // 用于统计已尝试的哈希次数
    let hash_count = Arc::new(AtomicU64::new(0));
    
    // 获取可用的CPU核心数量
    let num_threads = rayon::current_num_threads();
    println!("使用 {} 个线程进行挖矿", num_threads);
    
    // 每个线程负责不同范围的nonce
    let chunk_size = u64::MAX / num_threads as u64;
    
    let report_hash_count = hash_count.clone();
    let report_found = found.clone();
    let report_handle = thread::spawn(move || {
        let mut last_count = 0u64;
        let mut last_time = Instant::now();
        
        while !report_found.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_secs(1));
            let current_count = report_hash_count.load(Ordering::Relaxed);
            let current_time = Instant::now();
            let elapsed = current_time.duration_since(last_time).as_secs_f64();
            let hashes_per_second = (current_count - last_count) as f64 / elapsed;
            
            println!("当前速率: {:.2} 哈希/秒，总计尝试: {} 哈希", 
                     hashes_per_second, current_count);
            
            last_count = current_count;
            last_time = current_time;
        }
    });
    
    // 创建线程池并行处理
    rayon::scope(|s| {
        for i in 0..num_threads {
            let sender = sender.clone();
            let found = found.clone();
            let hash_count = hash_count.clone();
            let prefix = prefix.clone();
            let start = i as u64 * chunk_size;
            let end = if i == num_threads - 1 {
                u64::MAX
            } else {
                (i as u64 + 1) * chunk_size - 1
            };
            
            s.spawn(move |_| {
                mine_range(&prefix, difficulty, start, end, sender, found, hash_count);
            });
        }
    });
    
    // 主线程接收结果
    if let Ok((nonce, hash)) = receiver.recv() {
        let duration = start_time.elapsed();
        let total_hashes = hash_count.load(Ordering::Relaxed);
        
        println!("\n找到满足条件的nonce: {}", nonce);
        println!("对应的哈希值: {}", hash);
        println!("耗时: {:.2?}", duration);
        println!("总计尝试: {} 哈希", total_hashes);
        println!("平均哈希速率: {:.2} 哈希/秒", total_hashes as f64 / duration.as_secs_f64());
        
        // 显示组合字符串
        println!("组合字符串: {}{}", prefix, nonce);
    }
    
    // 等待报告线程结束
    let _ = report_handle.join();
}

fn mine_range(
    prefix: &str, 
    difficulty: usize, 
    start: u64, 
    end: u64, 
    sender: mpsc::Sender<(u64, String)>, 
    found: Arc<AtomicBool>,
    hash_count: Arc<AtomicU64>
) {
    let target_prefix = "0".repeat(difficulty);
    
    let mut nonce = start;
    let mut hasher = Sha256::new();
    let mut local_hash_count = 0u64;
    
    while nonce <= end && !found.load(Ordering::Relaxed) {
        hasher.reset();
        
        // 计算 prefix + nonce 的哈希值
        let data = format!("{}{}", prefix, nonce);
        hasher.update(data.as_bytes());
        
        // 获取哈希结果
        let result = hasher.finalize_reset();
        let hash_hex = hex::encode(result);
        
        // 检查前导零
        if hash_hex.starts_with(&target_prefix) {
            // 找到符合条件的nonce
            found.store(true, Ordering::Relaxed);
            let _ = sender.send((nonce, hash_hex));
            break;
        }
        
        nonce += 1;
        local_hash_count += 1;
        
        // 周期性更新计数和检查是否已经找到结果
        if local_hash_count % 100_000 == 0 {
            hash_count.fetch_add(local_hash_count, Ordering::Relaxed);
            local_hash_count = 0;
            
            if found.load(Ordering::Relaxed) {
                break;
            }
        }
    }
    
    // 添加剩余的哈希计数
    if local_hash_count > 0 {
        hash_count.fetch_add(local_hash_count, Ordering::Relaxed);
    }
}
