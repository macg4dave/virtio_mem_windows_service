use crate::service_loop::MemoryStateProvider;
use crate::virtio_mem::VirtioMemState;
use crate::virtio_mem_xml::parse_virtio_mem_xml;

pub trait VirtioMemXmlSource {
    fn read_xml(&mut self) -> Result<String, String>;
}

#[derive(Debug)]
pub struct XmlMemoryStateProvider<S> {
    source: S,
}

impl<S> XmlMemoryStateProvider<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S> MemoryStateProvider for XmlMemoryStateProvider<S>
where
    S: VirtioMemXmlSource,
{
    fn memory_state(&mut self) -> Result<VirtioMemState, String> {
        let xml = self.source.read_xml()?;
        parse_virtio_mem_xml(&xml)
            .map(|snapshot| snapshot.memory)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = "<domain><memory model='virtio-mem'><target><size unit='MiB'>8</size><block unit='MiB'>2</block><requested unit='MiB'>4</requested><current unit='MiB'>4</current></target><alias name='ua-virtio-mem0'/></memory></domain>";

    struct StubSource {
        result: Result<String, String>,
    }

    impl VirtioMemXmlSource for StubSource {
        fn read_xml(&mut self) -> Result<String, String> {
            self.result.clone()
        }
    }

    #[test]
    fn reads_and_validates_xml_state() {
        let mut provider = XmlMemoryStateProvider::new(StubSource {
            result: Ok(XML.to_owned()),
        });

        let state = provider.memory_state().expect("XML state should be valid");
        assert_eq!(state.size_bytes, 8 * 1024 * 1024);
        assert_eq!(state.block_size_bytes, 2 * 1024 * 1024);
        assert_eq!(state.requested_bytes, 4 * 1024 * 1024);
        assert_eq!(state.current_bytes, 4 * 1024 * 1024);
    }

    #[test]
    fn preserves_source_and_parse_errors() {
        let mut source_error = XmlMemoryStateProvider::new(StubSource {
            result: Err("source unavailable".to_owned()),
        });
        assert_eq!(
            source_error.memory_state(),
            Err("source unavailable".to_owned())
        );

        let mut parse_error = XmlMemoryStateProvider::new(StubSource {
            result: Ok("not XML".to_owned()),
        });
        assert!(parse_error.memory_state().is_err());
    }
}
