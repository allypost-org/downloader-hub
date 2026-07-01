#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("Database initialized failed: {0}")]
    Init(anyhow::Error),

    #[error("Database already initialized")]
    AlreadyInitialized,

    #[error("Failed to connect to database: {0}")]
    FailedToConnect(anyhow::Error),

    #[error("Database error: {0}")]
    Base(anyhow::Error),

    #[error("Database response error: {0}")]
    Response(ResponseError),

    #[error("Failed to serialize row: {0}")]
    SerializeToString(serde_json::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ResponseError {
    #[error("Failed to deserialize database result: {0}")]
    Deserialize(#[from] serde_json::Error),

    #[error("Got error response from database: {0}")]
    Response(String),

    #[error("Got convex error from response: {0}")]
    ConvexError(#[from] convex::ConvexError),
}
