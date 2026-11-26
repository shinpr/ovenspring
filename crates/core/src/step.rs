//! The steps the factory files, each one agent launch with its own prompt
//! (`factory/steps/<step>.md`), and what the flow needs to know about each.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::config::Role;

/// Serialized by its name, the same as [`Step::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Step {
    Requirements,
    Prd,
    PrdReview,
    CodebaseAnalysis,
    ScopeCheck,
    Prototype,
    UiAnalysis,
    UiSpec,
    UiSpecReview,
    Adr,
    AdrReview,
    Design,
    DesignVerify,
    DesignReview,
    DesignSync,
    TestDesign,
    WorkPlan,
    PlanReview,
    Decompose,
    Implement,
    TestReview,
    Quality,
    CodeReview,
    SecurityReview,
    RerunCheck,
    ReviewResponse,
    FixCi,
    MergeBase,
    Exception,
    SprintPlan,
    Facts,
    SprintReview,
    AgreementsCheck,
    Conversation,
    RunCommands,
}

impl Step {
    pub const ALL: [Step; 35] = [
        Step::Requirements,
        Step::Prd,
        Step::PrdReview,
        Step::CodebaseAnalysis,
        Step::ScopeCheck,
        Step::Prototype,
        Step::UiAnalysis,
        Step::UiSpec,
        Step::UiSpecReview,
        Step::Adr,
        Step::AdrReview,
        Step::Design,
        Step::DesignVerify,
        Step::DesignReview,
        Step::DesignSync,
        Step::TestDesign,
        Step::WorkPlan,
        Step::PlanReview,
        Step::Decompose,
        Step::Implement,
        Step::TestReview,
        Step::Quality,
        Step::CodeReview,
        Step::SecurityReview,
        Step::RerunCheck,
        Step::ReviewResponse,
        Step::FixCi,
        Step::MergeBase,
        Step::Exception,
        Step::SprintPlan,
        Step::Facts,
        Step::SprintReview,
        Step::AgreementsCheck,
        Step::Conversation,
        Step::RunCommands,
    ];

    /// The step whose file under `factory/steps/` holds this step's text. Steps that play one
    /// source agent share its file; what differs between them is in their items' requests.
    #[must_use]
    pub fn text_file(self) -> Step {
        match self {
            Step::ReviewResponse => Step::Implement,
            Step::FixCi | Step::MergeBase => Step::Quality,
            Step::Requirements
            | Step::Prd
            | Step::PrdReview
            | Step::CodebaseAnalysis
            | Step::ScopeCheck
            | Step::Prototype
            | Step::UiAnalysis
            | Step::UiSpec
            | Step::UiSpecReview
            | Step::Adr
            | Step::AdrReview
            | Step::Design
            | Step::DesignVerify
            | Step::DesignReview
            | Step::DesignSync
            | Step::TestDesign
            | Step::WorkPlan
            | Step::PlanReview
            | Step::Decompose
            | Step::Implement
            | Step::TestReview
            | Step::Quality
            | Step::CodeReview
            | Step::SecurityReview
            | Step::RerunCheck
            | Step::Exception
            | Step::SprintPlan
            | Step::Facts
            | Step::SprintReview
            | Step::AgreementsCheck
            | Step::Conversation
            | Step::RunCommands => self,
        }
    }

    /// The step's name: how items and records name it, and its file's name when it has one.
    pub fn as_str(self) -> &'static str {
        match self {
            Step::Requirements => "requirements",
            Step::Prd => "prd",
            Step::PrdReview => "prd-review",
            Step::CodebaseAnalysis => "codebase-analysis",
            Step::ScopeCheck => "scope-check",
            Step::Prototype => "prototype",
            Step::UiAnalysis => "ui-analysis",
            Step::UiSpec => "ui-spec",
            Step::UiSpecReview => "ui-spec-review",
            Step::Adr => "adr",
            Step::AdrReview => "adr-review",
            Step::Design => "design",
            Step::DesignVerify => "design-verify",
            Step::DesignReview => "design-review",
            Step::DesignSync => "design-sync",
            Step::TestDesign => "test-design",
            Step::WorkPlan => "work-plan",
            Step::PlanReview => "plan-review",
            Step::Decompose => "decompose",
            Step::Implement => "implement",
            Step::TestReview => "test-review",
            Step::Quality => "quality",
            Step::CodeReview => "code-review",
            Step::SecurityReview => "security-review",
            Step::RerunCheck => "rerun-check",
            Step::ReviewResponse => "review-response",
            Step::FixCi => "fix-ci",
            Step::MergeBase => "merge-base",
            Step::Exception => "exception",
            Step::SprintPlan => "sprint-plan",
            Step::Facts => "facts",
            Step::SprintReview => "sprint-review",
            Step::AgreementsCheck => "agreements-check",
            Step::Conversation => "conversation",
            Step::RunCommands => "run-commands",
        }
    }

    /// The role whose CLI, model and agreements a launch of the step takes.
    pub fn role(self) -> Role {
        match self {
            Step::TestDesign
            | Step::Implement
            | Step::ReviewResponse
            | Step::Quality
            | Step::FixCi
            | Step::MergeBase => Role::Executor,
            Step::PrdReview
            | Step::UiSpecReview
            | Step::AdrReview
            | Step::DesignVerify
            | Step::DesignReview
            | Step::DesignSync
            | Step::PlanReview
            | Step::TestReview
            | Step::CodeReview
            | Step::SecurityReview
            | Step::AgreementsCheck => Role::Reviewer,
            Step::Facts => Role::Analyzer,
            Step::Requirements
            | Step::Prd
            | Step::CodebaseAnalysis
            | Step::ScopeCheck
            | Step::Prototype
            | Step::UiAnalysis
            | Step::UiSpec
            | Step::Adr
            | Step::Design
            | Step::WorkPlan
            | Step::Decompose
            | Step::RerunCheck
            | Step::Exception
            | Step::SprintPlan
            | Step::SprintReview
            | Step::Conversation
            | Step::RunCommands => Role::Lead,
        }
    }

    /// The document the step writes, for an author step.
    pub fn document(self) -> Option<Document> {
        match self {
            Step::Prd => Some(Document::Prd),
            Step::UiSpec => Some(Document::UiSpec),
            Step::Adr => Some(Document::Adr),
            Step::Design => Some(Document::Design),
            Step::WorkPlan => Some(Document::WorkPlan),
            Step::Requirements
            | Step::PrdReview
            | Step::CodebaseAnalysis
            | Step::ScopeCheck
            | Step::Prototype
            | Step::UiAnalysis
            | Step::UiSpecReview
            | Step::AdrReview
            | Step::DesignVerify
            | Step::DesignReview
            | Step::DesignSync
            | Step::TestDesign
            | Step::PlanReview
            | Step::Decompose
            | Step::Implement
            | Step::TestReview
            | Step::Quality
            | Step::CodeReview
            | Step::SecurityReview
            | Step::RerunCheck
            | Step::ReviewResponse
            | Step::FixCi
            | Step::MergeBase
            | Step::Exception
            | Step::SprintPlan
            | Step::Facts
            | Step::SprintReview
            | Step::AgreementsCheck
            | Step::Conversation
            | Step::RunCommands => None,
        }
    }

    /// The check the step is, for a check whose applied findings are corrected and rechecked
    /// before the issue goes on (the review round's checks run as a round instead).
    pub fn check(self) -> Option<Check> {
        match self {
            Step::PrdReview => Some(Check::PrdReview),
            Step::UiSpecReview => Some(Check::UiSpecReview),
            Step::AdrReview => Some(Check::AdrReview),
            Step::DesignVerify => Some(Check::DesignVerify),
            Step::DesignReview => Some(Check::DesignReview),
            Step::DesignSync => Some(Check::DesignSync),
            Step::PlanReview => Some(Check::PlanReview),
            Step::TestReview => Some(Check::TestReview),
            Step::Requirements
            | Step::Prd
            | Step::CodebaseAnalysis
            | Step::ScopeCheck
            | Step::Prototype
            | Step::UiAnalysis
            | Step::UiSpec
            | Step::Adr
            | Step::Design
            | Step::TestDesign
            | Step::WorkPlan
            | Step::Decompose
            | Step::Implement
            | Step::Quality
            | Step::CodeReview
            | Step::SecurityReview
            | Step::RerunCheck
            | Step::ReviewResponse
            | Step::FixCi
            | Step::MergeBase
            | Step::Exception
            | Step::SprintPlan
            | Step::Facts
            | Step::SprintReview
            | Step::AgreementsCheck
            | Step::Conversation
            | Step::RunCommands => None,
        }
    }

    /// A check runs in rounds: a recheck after a correction is its next round.
    pub fn is_check(self) -> bool {
        self.check().is_some() || matches!(self, Step::CodeReview | Step::SecurityReview)
    }
}

/// A document an author step writes and its checks review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Document {
    Prd,
    UiSpec,
    Adr,
    Design,
    WorkPlan,
}

impl Document {
    pub fn step(self) -> Step {
        match self {
            Document::Prd => Step::Prd,
            Document::UiSpec => Step::UiSpec,
            Document::Adr => Step::Adr,
            Document::Design => Step::Design,
            Document::WorkPlan => Step::WorkPlan,
        }
    }

    pub fn first_check(self) -> Check {
        match self {
            Document::Prd => Check::PrdReview,
            Document::UiSpec => Check::UiSpecReview,
            Document::Adr => Check::AdrReview,
            Document::Design => Check::DesignReview,
            Document::WorkPlan => Check::PlanReview,
        }
    }

    /// The document as a request names it.
    pub fn name(self) -> &'static str {
        match self {
            Document::Prd => "PRD",
            Document::UiSpec => "UI Spec",
            Document::Adr => "ADRs",
            Document::Design => "Design Doc",
            Document::WorkPlan => "Work Plan",
        }
    }
}

/// A check whose applied findings its corrector takes before the check runs again. Serialized by
/// its step's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Check {
    PrdReview,
    UiSpecReview,
    AdrReview,
    DesignVerify,
    DesignReview,
    DesignSync,
    PlanReview,
    TestReview,
}

impl Check {
    pub fn step(self) -> Step {
        match self {
            Check::PrdReview => Step::PrdReview,
            Check::UiSpecReview => Step::UiSpecReview,
            Check::AdrReview => Step::AdrReview,
            Check::DesignVerify => Step::DesignVerify,
            Check::DesignReview => Step::DesignReview,
            Check::DesignSync => Step::DesignSync,
            Check::PlanReview => Step::PlanReview,
            Check::TestReview => Step::TestReview,
        }
    }

    /// The document the check reviews, whose author corrects its applied findings; the test
    /// review's are the executor's.
    pub fn document(self) -> Option<Document> {
        match self {
            Check::PrdReview => Some(Document::Prd),
            Check::UiSpecReview => Some(Document::UiSpec),
            Check::AdrReview => Some(Document::Adr),
            Check::DesignVerify | Check::DesignReview | Check::DesignSync => Some(Document::Design),
            Check::PlanReview => Some(Document::WorkPlan),
            Check::TestReview => None,
        }
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Step {
    type Err = String;

    fn from_str(name: &str) -> Result<Step, String> {
        Step::ALL
            .into_iter()
            .find(|s| s.as_str() == name)
            .ok_or_else(|| format!("no step is named {name:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_and_a_document_name_their_own_step() {
        for step in Step::ALL {
            if let Some(check) = step.check() {
                assert_eq!(check.step(), step);
                assert_eq!(
                    serde_json::to_value(check).unwrap(),
                    serde_json::json!(step.as_str())
                );
            }
            if let Some(document) = step.document() {
                assert_eq!(document.step(), step);
            }
        }
    }

    #[test]
    fn a_step_is_serialized_by_its_name() {
        for step in Step::ALL {
            assert_eq!(
                serde_json::to_value(step).unwrap(),
                serde_json::json!(step.as_str())
            );
            assert_eq!(step.as_str().parse::<Step>(), Ok(step));
        }
    }
}
