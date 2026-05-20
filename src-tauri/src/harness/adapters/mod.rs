use crate::harness::case::HarnessSubject;

pub mod agent_loop;
pub mod browser;
pub mod memory;

pub trait HarnessAdapter: Send + Sync {
    fn subject(&self) -> HarnessSubject;
    fn adapter_id(&self) -> &'static str;
}

pub const AGENT_LOOP_ADAPTER_ID: &str = "agent_loop";
pub const BROWSER_ADAPTER_ID: &str = "browser";
pub const TOOLS_ADAPTER_ID: &str = "tools";
pub const PERMISSIONS_ADAPTER_ID: &str = "permissions";
pub const HOOKS_ADAPTER_ID: &str = "hooks";
pub const MEMORY_ADAPTER_ID: &str = "memory";
pub const GBRAIN_ADAPTER_ID: &str = "gbrain";
pub const SKILLS_ADAPTER_ID: &str = "skills";
pub const TASKS_ADAPTER_ID: &str = "tasks";
pub const PROMPTS_ADAPTER_ID: &str = "prompts";
pub const COORDINATOR_ADAPTER_ID: &str = "coordinator";

#[cfg(test)]
mod tests {
    use super::*;

    struct TestAdapter;

    impl HarnessAdapter for TestAdapter {
        fn subject(&self) -> HarnessSubject {
            HarnessSubject::Gbrain
        }

        fn adapter_id(&self) -> &'static str {
            GBRAIN_ADAPTER_ID
        }
    }

    #[test]
    fn adapter_trait_names_subject_and_id() {
        let adapter = TestAdapter;
        assert_eq!(adapter.subject(), HarnessSubject::Gbrain);
        assert_eq!(adapter.adapter_id(), "gbrain");
    }
}
