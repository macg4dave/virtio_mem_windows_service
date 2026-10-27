//! RHEL host-side adapters for guest-driven virtio-mem resizing.

pub mod attestation;
pub mod checkpoint;
pub mod cli;
pub mod compatibility_source;
pub mod config;
pub mod guest_channel;
pub mod host_memory;
pub mod ownership;
pub mod qga;
pub mod resize_sink;
pub mod runtime;
pub mod virsh;
pub mod xml_source;
