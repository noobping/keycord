//! Standard-library-only background worker construction.

use std::io;
use std::thread;

const WORKER_STACK_SIZE_BYTES: usize = 4 * 1024 * 1024;

/// Spawns a named Keycord worker with the application worker stack size.
pub fn spawn_worker<T, Task>(name: &str, task: Task) -> io::Result<thread::JoinHandle<T>>
where
    T: Send + 'static,
    Task: FnOnce() -> T + Send + 'static,
{
    thread::Builder::new()
        .name(format!("keycord-{name}"))
        .stack_size(WORKER_STACK_SIZE_BYTES)
        .spawn(task)
}

/// Spawns a named worker and panics with a useful message if the OS rejects it.
pub fn spawn_worker_or_panic<T, Task>(name: &str, task: Task) -> thread::JoinHandle<T>
where
    T: Send + 'static,
    Task: FnOnce() -> T + Send + 'static,
{
    spawn_worker(name, task)
        .unwrap_or_else(|err| panic!("Failed to spawn background worker '{name}': {err}"))
}

#[cfg(test)]
mod tests {
    use super::spawn_worker;

    #[test]
    fn workers_use_keycord_thread_names() {
        let name = spawn_worker("test-worker", || {
            std::thread::current().name().map(str::to_owned)
        })
        .expect("worker should spawn")
        .join()
        .expect("worker should finish");

        assert_eq!(name.as_deref(), Some("keycord-test-worker"));
    }
}
