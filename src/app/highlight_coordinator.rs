use std::hash::{Hash, Hasher};

use lazydiff_diffs::DiffMode;
use ratatui::layout::Rect;

use crate::highlight_daemon::HighlightLineWindow;

use super::DiffSource;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct DiffGeneration(pub(crate) u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HighlightToken {
    pub(crate) route: DiffSource,
    pub(crate) generation: DiffGeneration,
    pub(crate) request_id: u64,
    pub(crate) window_key: HighlightWindowKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HighlightWindowKey {
    route_id: String,
    generation: DiffGeneration,
    mode: DiffMode,
    viewport_width: u16,
    viewport_height: u16,
    visual_start: usize,
    visual_end: usize,
    overscan: usize,
    inline_layout_hash: u64,
    file_indices: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HighlightFileJob {
    pub(crate) file_index: usize,
    pub(crate) old_path: Option<String>,
    pub(crate) new_path: String,
    pub(crate) old_line_window: Option<HighlightLineWindow>,
    pub(crate) new_line_window: Option<HighlightLineWindow>,
}

#[derive(Clone, Debug)]
pub(crate) struct HighlightRequestEnvelope {
    pub(crate) token: HighlightToken,
    pub(crate) jobs: Vec<HighlightFileJob>,
    pub(crate) visible_job_count: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct HighlightFrameWindow {
    pub(crate) route: DiffSource,
    pub(crate) mode: DiffMode,
    pub(crate) viewport: Rect,
    pub(crate) visual_start: usize,
    pub(crate) visual_end: usize,
    pub(crate) overscan: usize,
    pub(crate) inline_layout_hash: u64,
    pub(crate) file_indices: Vec<usize>,
    pub(crate) jobs: Vec<HighlightFileJob>,
    pub(crate) visible_job_count: usize,
}

#[derive(Debug, Default)]
pub(crate) struct HighlightCoordinator {
    generation: DiffGeneration,
    request_counter: u64,
    latest_applied_request: u64,
    last_window_key: Option<HighlightWindowKey>,
}

impl HighlightCoordinator {
    pub(crate) fn document_replaced(&mut self) {
        self.generation.0 = self.generation.0.saturating_add(1);
        self.latest_applied_request = 0;
        self.last_window_key = None;
    }

    pub(crate) fn visible_window_changed(
        &mut self,
        window: HighlightFrameWindow,
    ) -> Option<HighlightRequestEnvelope> {
        if window.jobs.is_empty() {
            return None;
        }

        let window_key = HighlightWindowKey {
            route_id: window.route.session_id(),
            generation: self.generation,
            mode: window.mode,
            viewport_width: window.viewport.width,
            viewport_height: window.viewport.height,
            visual_start: window.visual_start,
            visual_end: window.visual_end,
            overscan: window.overscan,
            inline_layout_hash: window.inline_layout_hash,
            file_indices: window.file_indices,
        };
        if self.last_window_key.as_ref() == Some(&window_key) {
            return None;
        }
        self.last_window_key = Some(window_key.clone());
        self.request_counter = self.request_counter.saturating_add(1);
        Some(HighlightRequestEnvelope {
            token: HighlightToken {
                route: window.route,
                generation: self.generation,
                request_id: self.request_counter,
                window_key,
            },
            jobs: window.jobs,
            visible_job_count: window.visible_job_count,
        })
    }

    pub(crate) fn should_apply(
        &mut self,
        token: &HighlightToken,
        current_route: &DiffSource,
    ) -> bool {
        if &token.route != current_route || token.generation != self.generation {
            return false;
        }
        if token.request_id != self.request_counter
            || token.request_id < self.latest_applied_request
        {
            return false;
        }
        self.latest_applied_request = token.request_id;
        true
    }
}

pub(crate) fn inline_layout_hash<I, T>(items: I) -> u64
where
    I: IntoIterator<Item = T>,
    T: Hash,
{
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for item in items {
        item.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::LocalWorktreeRoute;

    fn route() -> DiffSource {
        DiffSource::LocalWorktree(LocalWorktreeRoute {
            repo_path: "/repo".to_string(),
            branch: "main".to_string(),
            base_ref: "HEAD".to_string(),
        })
    }

    fn window(route: DiffSource) -> HighlightFrameWindow {
        HighlightFrameWindow {
            route,
            mode: DiffMode::Split,
            viewport: Rect::new(0, 0, 100, 24),
            visual_start: 0,
            visual_end: 24,
            overscan: 24,
            inline_layout_hash: 0,
            file_indices: vec![0],
            jobs: vec![HighlightFileJob {
                file_index: 0,
                old_path: Some("a.rs".to_string()),
                new_path: "a.rs".to_string(),
                old_line_window: None,
                new_line_window: None,
            }],
            visible_job_count: 1,
        }
    }

    #[test]
    fn same_window_is_deduped_until_document_generation_changes() {
        let route = route();
        let mut coordinator = HighlightCoordinator::default();

        assert!(
            coordinator
                .visible_window_changed(window(route.clone()))
                .is_some()
        );
        assert!(
            coordinator
                .visible_window_changed(window(route.clone()))
                .is_none()
        );

        coordinator.document_replaced();

        assert!(coordinator.visible_window_changed(window(route)).is_some());
    }

    #[test]
    fn stale_generation_result_is_rejected_for_same_route() {
        let route = route();
        let mut coordinator = HighlightCoordinator::default();
        let token = coordinator
            .visible_window_changed(window(route.clone()))
            .expect("request")
            .token;

        coordinator.document_replaced();

        assert!(!coordinator.should_apply(&token, &route));
    }

    #[test]
    fn older_request_result_is_rejected_after_newer_apply() {
        let route = route();
        let mut coordinator = HighlightCoordinator::default();
        let old = coordinator
            .visible_window_changed(window(route.clone()))
            .expect("old request")
            .token;
        let mut newer_window = window(route.clone());
        newer_window.visual_start = 10;
        newer_window.visual_end = 34;
        let newer = coordinator
            .visible_window_changed(newer_window)
            .expect("newer request")
            .token;

        assert!(coordinator.should_apply(&newer, &route));
        assert!(!coordinator.should_apply(&old, &route));
    }

    #[test]
    fn older_request_result_is_rejected_after_newer_request_is_issued() {
        let route = route();
        let mut coordinator = HighlightCoordinator::default();
        let old = coordinator
            .visible_window_changed(window(route.clone()))
            .expect("old request")
            .token;
        let mut newer_window = window(route.clone());
        newer_window.visual_start = 20;
        newer_window.visual_end = 44;
        let _newer = coordinator
            .visible_window_changed(newer_window)
            .expect("newer request")
            .token;

        assert!(!coordinator.should_apply(&old, &route));
    }
}
