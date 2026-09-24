//! Validated domain values. Construction succeeds only for contract-valid data.

mod activation;
mod candidate;
mod environment;
mod error;
mod ids;
mod intent;

pub use activation::{ActivationId, InvalidActivationId, MAX_ACTIVATION_SEQUENCE};
pub use candidate::{
    CandidatePath, CandidateSet, CandidateSetDigest, CandidateSetFingerprint, ResolutionSeed,
};
pub use environment::{
    BackgroundBlurIntensity, Color, ColorsManifest, CursorStyle, EnvironmentManifest,
    FontSizeMillipoints, ImageWallpaper, MediaType, OpacityMillionths, TerminalManifest,
    WallpaperFit, WallpaperManifest, WallpaperPosition,
};
pub use error::ValidationError;
pub use ids::{EnvironmentId, Sha256Digest};
pub use intent::{
    ColorsIntent, ConfigIntent, IntentId, ProfileIntent, SourceIntent, SourcePath, TerminalIntent,
    WallpaperIntent, WallpaperSelection,
};
