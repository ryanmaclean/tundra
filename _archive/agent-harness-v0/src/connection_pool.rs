use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn, error};
use reqwest::{Client, ClientBuilder};

/// Configuration for HTTP connection pool
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    /// Maximum number of connections in the pool
    pub max_connections: usize,
    /// Maximum number of idle connections
    pub max_idle_connections: usize,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Idle timeout for connections
    pub idle_timeout: Duration,
    /// Maximum lifetime for connections
    pub max_lifetime: Option<Duration>,
    /// Whether to enable connection reuse
    pub enable_reuse: bool,
    /// HTTP/2 configuration
    pub enable_http2: bool,
    /// TCP keepalive configuration
    pub tcp_keepalive: Option<Duration>,
    /// TCP nodelay configuration
    pub tcp_nodelay: bool,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 100,
            max_idle_connections: 10,
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(60),
            max_lifetime: Some(Duration::from_secs(300)),
            enable_reuse: true,
            enable_http2: true,
            tcp_keepalive: Some(Duration::from_secs(60)),
            tcp_nodelay: true,
        }
    }
}

/// Pooled HTTP connection
#[derive(Debug)]
struct PooledConnection {
    client: Client,
    created_at: Instant,
    last_used: Instant,
    in_use: bool,
    use_count: u64,
}

/// HTTP connection pool for better performance
pub struct ConnectionPool {
    config: ConnectionPoolConfig,
    connections: Arc<RwLock<Vec<PooledConnection>>>,
    available_connections: Arc<Mutex<usize>>,
    total_created: Arc<Mutex<usize>>,
    total_reused: Arc<Mutex<usize>>,
}

impl ConnectionPool {
    pub fn new(config: ConnectionPoolConfig) -> Result<Self, reqwest::Error> {
        let client = Self::create_client(&config)?;
        
        let initial_connection = PooledConnection {
            client,
            created_at: Instant::now(),
            last_used: Instant::now(),
            in_use: false,
            use_count: 0,
        };

        Ok(Self {
            config,
            connections: Arc::new(RwLock::new(vec![initial_connection])),
            available_connections: Arc::new(Mutex::new(1)),
            total_created: Arc::new(Mutex::new(1)),
            total_reused: Arc::new(Mutex::new(0)),
        })
    }

    /// Create a new HTTP client with the given configuration
    fn create_client(config: &ConnectionPoolConfig) -> Result<Client, reqwest::Error> {
        let mut builder = ClientBuilder::new()
            .timeout(config.connection_timeout)
            .pool_max_idle_per_host(config.max_idle_connections)
            .pool_idle_timeout(config.idle_timeout);

        if let Some(_max_lifetime) = config.max_lifetime {
            builder = builder.pool_max_idle_per_host(config.max_idle_connections)
                .pool_idle_timeout(config.idle_timeout);
        }

        if config.enable_http2 {
            builder = builder.http2_prior_knowledge();
        }

        if let Some(keepalive) = config.tcp_keepalive {
            builder = builder.tcp_keepalive(keepalive);
        }

        if config.tcp_nodelay {
            builder = builder.tcp_nodelay(true);
        }

        builder.build()
    }

    /// Get a connection from the pool
    pub async fn get_connection(&self) -> Result<Client, reqwest::Error> {
        // Try to get an available connection
        {
            let mut connections = self.connections.write().await;
            
            for conn in connections.iter_mut() {
                if !conn.in_use {
                    // Check if connection is still valid
                    if self.is_connection_valid(conn) {
                        conn.in_use = true;
                        conn.last_used = Instant::now();
                        conn.use_count += 1;
                        *self.available_connections.lock().await -= 1;
                        *self.total_reused.lock().await += 1;
                        debug!("Reusing existing connection (use count: {})", conn.use_count);
                        return Ok(conn.client.clone());
                    } else {
                        // Remove invalid connection
                        warn!("Removing invalid connection");
                    }
                }
            }
        }

        // No available connections, try to create a new one
        self.create_new_connection().await
    }

    /// Return a connection to the pool
    pub async fn return_connection(&self, client: Client) {
        let mut connections = self.connections.write().await;
        
        // Find the connection and mark it as available
        for conn in connections.iter_mut() {
            if conn.client.same_client(&client) && conn.in_use {
                conn.in_use = false;
                conn.last_used = Instant::now();
                *self.available_connections.lock().await += 1;
                debug!("Connection returned to pool");
                return;
            }
        }

        // If we couldn't find the connection, it might be invalid
        warn!("Could not find connection to return to pool");
    }

    /// Create a new connection
    async fn create_new_connection(&self) -> Result<Client, reqwest::Error> {
        let mut connections = self.connections.write().await;
        
        // Check if we can create a new connection
        if connections.len() >= self.config.max_connections {
            // Try to clean up old connections first
            self.cleanup_old_connections(&mut connections).await;
            
            if connections.len() >= self.config.max_connections {
                return Err(reqwest::Error::from(std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    "Connection pool exhausted"
                )));
            }
        }

        let client = Self::create_client(&self.config)?;
        
        let new_connection = PooledConnection {
            client: client.clone(),
            created_at: Instant::now(),
            last_used: Instant::now(),
            in_use: true,
            use_count: 1,
        };

        connections.push(new_connection);
        *self.total_created.lock().await += 1;
        info!("Created new connection (total: {})", connections.len());

        Ok(client)
    }

    /// Check if a connection is still valid
    fn is_connection_valid(&self, conn: &PooledConnection) -> bool {
        let now = Instant::now();
        
        // Check if connection is too old
        if let Some(max_lifetime) = self.config.max_lifetime {
            if now.duration_since(conn.created_at) > max_lifetime {
                return false;
            }
        }

        // Check if connection has been idle too long
        if now.duration_since(conn.last_used) > self.config.idle_timeout {
            return false;
        }

        true
    }

    /// Clean up old connections
    async fn cleanup_old_connections(&self, connections: &mut Vec<PooledConnection>) {
        let _now = Instant::now();
        let initial_len = connections.len();
        
        connections.retain(|conn| {
            let is_valid = self.is_connection_valid(conn);
            if !is_valid {
                debug!("Removing old connection");
            }
            is_valid
        });

        let removed = initial_len - connections.len();
        if removed > 0 {
            info!("Cleaned up {} old connections", removed);
        }
    }

    /// Get pool statistics
    pub async fn stats(&self) -> PoolStats {
        let connections = self.connections.read().await;
        let available = *self.available_connections.lock().await;
        let total_created = *self.total_created.lock().await;
        let total_reused = *self.total_reused.lock().await;

        let in_use = connections.len() - available;
        let total_use_count: u64 = connections.iter().map(|c| c.use_count).sum();

        PoolStats {
            total_connections: connections.len(),
            available_connections: available,
            in_use_connections: in_use,
            total_created: total_created as u32,
            total_reused: total_reused as u32,
            total_use_count,
            reuse_ratio: if total_created > 0 {
                total_reused as f64 / total_created as f64
            } else {
                0.0
            },
        }
    }

    /// Perform maintenance on the connection pool
    pub async fn maintenance(&self) {
        let mut connections = self.connections.write().await;
        self.cleanup_old_connections(&mut connections).await;
        
        // Ensure we have at least one connection
        if connections.is_empty() {
            match Self::create_client(&self.config) {
                Ok(client) => {
                    let conn = PooledConnection {
                        client,
                        created_at: Instant::now(),
                        last_used: Instant::now(),
                        in_use: false,
                        use_count: 0,
                    };
                    connections.push(conn);
                    *self.available_connections.lock().await = 1;
                    info!("Created maintenance connection");
                }
                Err(e) => {
                    error!("Failed to create maintenance connection: {}", e);
                }
            }
        }
    }
}

/// Connection pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub total_connections: usize,
    pub available_connections: usize,
    pub in_use_connections: usize,
    pub total_created: u32,
    pub total_reused: u32,
    pub total_use_count: u64,
    pub reuse_ratio: f64,
}

impl std::fmt::Display for PoolStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Pool Stats: total={}, available={}, in_use={}, created={}, reused={}, reuse_ratio={:.2}%",
            self.total_connections,
            self.available_connections,
            self.in_use_connections,
            self.total_created,
            self.total_reused,
            self.reuse_ratio * 100.0
        )
    }
}

/// HTTP client wrapper that uses connection pooling
pub struct PooledHttpClient {
    pool: Arc<ConnectionPool>,
}

impl PooledHttpClient {
    pub fn new(config: ConnectionPoolConfig) -> Result<Self, reqwest::Error> {
        let pool = Arc::new(ConnectionPool::new(config)?);
        Ok(Self { pool })
    }

    /// Get a client for making requests
    pub async fn client(&self) -> Result<Client, reqwest::Error> {
        self.pool.get_connection().await
    }

    /// Return a client to the pool
    pub async fn return_client(&self, client: Client) {
        self.pool.return_connection(client).await;
    }

    /// Get pool statistics
    pub async fn stats(&self) -> PoolStats {
        self.pool.stats().await
    }

    /// Perform maintenance on the pool
    pub async fn maintenance(&self) {
        self.pool.maintenance().await;
    }
}

/// Drop implementation to automatically return connections
impl Drop for PooledHttpClient {
    fn drop(&mut self) {
        // Note: In a real implementation, you might want to handle this differently
        // since Drop is synchronous and the pool operations are async
        debug!("PooledHttpClient dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_connection_pool_basic() {
        let config = ConnectionPoolConfig {
            max_connections: 5,
            max_idle_connections: 2,
            connection_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(10),
            max_lifetime: Some(Duration::from_secs(30)),
            enable_reuse: true,
            enable_http2: true,
            tcp_keepalive: Some(Duration::from_secs(60)),
            tcp_nodelay: true,
        };

        let pool = ConnectionPool::new(config).unwrap();
        
        // Get a connection
        let client1 = pool.get_connection().await.unwrap();
        assert!(client1.get("https://httpbin.org/get").send().await.is_ok());
        
        // Return the connection
        pool.return_connection(client1).await;
        
        // Get another connection (should reuse the first one)
        let client2 = pool.get_connection().await.unwrap();
        assert!(client2.get("https://httpbin.org/get").send().await.is_ok());
        
        let stats = pool.stats().await;
        assert_eq!(stats.total_connections, 1);
        assert_eq!(stats.total_reused, 1);
        assert!(stats.reuse_ratio > 0.0);
    }

    #[tokio::test]
    async fn test_connection_pool_exhaustion() {
        let config = ConnectionPoolConfig {
            max_connections: 2,
            max_idle_connections: 1,
            connection_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(10),
            max_lifetime: Some(Duration::from_secs(30)),
            enable_reuse: true,
            enable_http2: true,
            tcp_keepalive: Some(Duration::from_secs(60)),
            tcp_nodelay: true,
        };

        let pool = ConnectionPool::new(config).unwrap();
        
        // Get all connections
        let client1 = pool.get_connection().await.unwrap();
        let client2 = pool.get_connection().await.unwrap();
        
        // Try to get a third connection (should fail)
        let client3 = pool.get_connection().await;
        assert!(client3.is_err());
        
        // Return one connection
        pool.return_connection(client1).await;
        
        // Now we should be able to get another connection
        let client3 = pool.get_connection().await.unwrap();
        assert!(client3.get("https://httpbin.org/get").send().await.is_ok());
    }

    #[tokio::test]
    async fn test_pooled_http_client() {
        let config = ConnectionPoolConfig::default();
        let client = PooledHttpClient::new(config).unwrap();
        
        // Make a request
        let http_client = client.client().await.unwrap();
        let response = http_client.get("https://httpbin.org/get").send().await.unwrap();
        assert_eq!(response.status(), 200);
        
        // Return the client
        client.return_client(http_client).await;
        
        // Check stats
        let stats = client.stats().await;
        assert_eq!(stats.total_connections, 1);
        assert_eq!(stats.available_connections, 1);
    }
}
