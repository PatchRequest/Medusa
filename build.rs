// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Build script for the Medusa kernel driver.
//!
//! Configures linker flags and WDK settings via [`wdk_build::Config`].

fn main() -> Result<(), wdk_build::ConfigError> {
    wdk_build::configure_wdk_binary_build()
}
