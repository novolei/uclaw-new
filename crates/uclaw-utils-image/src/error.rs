// SPDX-License-Identifier: Apache-2.0
// Derived from codex-rs/utils/image/src/error.rs (https://github.com/openai/codex).
// Copyright (c) OpenAI. Licensed under Apache License 2.0.
// See NOTICE in the repository root.

use image::ImageError;
use image::ImageFormat;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImageProcessingError {
    #[error("failed to read image at {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to decode image at {path}: {source}")]
    Decode {
        path: PathBuf,
        #[source]
        source: image::ImageError,
    },
    #[error("failed to encode image as {format:?}: {source}")]
    Encode {
        format: ImageFormat,
        #[source]
        source: image::ImageError,
    },
    #[error("unsupported image `{mime}`")]
    UnsupportedImageFormat { mime: String },
}

impl ImageProcessingError {
    pub fn decode_error(path: &std::path::Path, source: image::ImageError) -> Self {
        if matches!(source, ImageError::Decoding(_)) {
            return ImageProcessingError::Decode {
                path: path.to_path_buf(),
                source,
            };
        }

        let mime = mime_guess::from_path(path)
            .first()
            .map(|mime_guess| mime_guess.essence_str().to_owned())
            .unwrap_or_else(|| "unknown".to_string());
        ImageProcessingError::UnsupportedImageFormat { mime }
    }

    pub fn is_invalid_image(&self) -> bool {
        matches!(
            self,
            ImageProcessingError::Decode {
                source: ImageError::Decoding(_),
                ..
            }
        )
    }
}
