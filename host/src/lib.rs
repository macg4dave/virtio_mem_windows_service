//! RHEL host-side adapters for the shared virtio-mem control policy.

pub mod attestation;
pub mod cli;
pub mod compatibility_source;
pub mod config;
pub mod dommemstat;
pub mod host_memory;
pub mod qga;
pub mod raw_telemetry;
pub mod resize_sink;
pub mod runtime;
pub mod virsh;
pub mod xml_source;
