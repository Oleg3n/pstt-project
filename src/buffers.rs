use std::sync::{Arc, Mutex, Condvar};
use std::collections::VecDeque;

pub struct BlockingQueue<T> {
    queue: Mutex<VecDeque<T>>,
    condvar: Condvar,
    max_size: usize,
}

impl<T> BlockingQueue<T> {
    pub fn new(max_size: usize) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(VecDeque::with_capacity(max_size)),
            condvar: Condvar::new(),
            max_size,
        })
    }
    
    pub fn push(&self, items: Vec<T>) -> bool {
        let mut queue = self.queue.lock().unwrap();
        
        if queue.len() + items.len() > self.max_size {
            log::warn!("Queue overflow! Dropping {} items", items.len());
            return false;
        }
        
        queue.extend(items);
        self.condvar.notify_one();
        true
    }
    
    pub fn try_pop_batch(&self, max_count: usize) -> Option<Vec<T>> {
        let mut queue = self.queue.lock().unwrap();
        
        if queue.is_empty() {
            return None;
        }
        
        let count = queue.len().min(max_count);
        Some(queue.drain(..count).collect())
    }
    
}

pub struct AudioPipeline {
    pub raw_queue: Arc<BlockingQueue<f32>>,
    pub resampled_queue_writer: Arc<BlockingQueue<f32>>,
    pub resampled_queue_vosk: Arc<BlockingQueue<f32>>,
}

impl AudioPipeline {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            raw_queue: BlockingQueue::new(buffer_size),
            resampled_queue_writer: BlockingQueue::new(buffer_size),
            resampled_queue_vosk: BlockingQueue::new(buffer_size),
        }
    }
}
