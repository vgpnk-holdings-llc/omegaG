//! Safe Codex Micro semantics and the transport boundary.
//!
//! No focused-window injection is permitted here. Mutations are fully bound to
//! app-server identities; the shipped transport rejects them observably.

use crate::config::CodexMicroConfig;
use crate::input::{ButtonState, DPad, UnifiedInput};
use std::collections::{HashSet, VecDeque};

pub const SLOT_COUNT: usize = 6;
pub const DOUBLE_PRESS_MS: u64 = 350; // inclusive: delta <= 350
const MAX_REPLAY_IDS: usize = 1024;
const MAX_THREADS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatStatus {
    #[default]
    Unassigned,
    Idle,
    Thinking,
    CompleteUnread,
    RequiresInput,
    Error,
}

impl ChatStatus {
    pub fn color(self) -> [u8; 3] {
        match self {
            Self::Idle => [0xFF, 0xFF, 0xFF],
            Self::Thinking => [0x3B, 0x82, 0xF6],
            Self::CompleteUnread => [0x22, 0xC5, 0x5E],
            Self::RequiresInput => [0xF5, 0x9E, 0x0B],
            Self::Error => [0xEF, 0x44, 0x44],
            Self::Unassigned => [0, 0, 0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourcePolicy {
    #[default]
    Recent,
    Pinned,
    Priority,
    Custom,
}

impl SourcePolicy {
    pub fn parse(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "pinned" => Self::Pinned,
            "priority" => Self::Priority,
            "custom" => Self::Custom,
            _ => Self::Recent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadContext {
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub item_id: Option<String>,
    pub approval_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadRecord {
    pub context: ThreadContext,
    pub status: ChatStatus,
    pub updated_ms: u64,
    pub pinned: bool,
    pub priority: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChatSlot {
    pub thread: Option<ThreadRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticAction {
    Activate,
    ToggleFast,
    Approve,
    Decline,
    ContinueInNewChat,
    PushToTalk { active: bool, latched: bool },
    Send,
    Cardinal { direction: DPad },
    SetReasoning(u8),
    Command(String),
    Skill(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundAction {
    pub action: SemanticAction,
    pub target: Option<ThreadContext>,
}

impl PartialEq<SemanticAction> for BoundAction {
    fn eq(&self, other: &SemanticAction) -> bool {
        &self.action == other
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MutationIdentity {
    pub connection_generation: u64,
    pub request_id: u64,
    pub method: String,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub item_id: Option<String>,
    pub approval_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutation {
    pub identity: MutationIdentity,
    pub action: SemanticAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    Unavailable,
    UnassignedTarget,
    StaleGeneration,
    DuplicateMutation,
    ContextMismatch,
    OutOfOrderEvent,
}

pub trait CodexTransport: Send {
    fn mutate(&mut self, mutation: &Mutation) -> Result<(), TransportError>;
}

/// Shipped until a documented, capability-pinned app-server client exists.
pub struct UnavailableTransport;
impl CodexTransport for UnavailableTransport {
    fn mutate(&mut self, _: &Mutation) -> Result<(), TransportError> {
        Err(TransportError::Unavailable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchResult {
    Applied(MutationIdentity),
    Rejected {
        action: SemanticAction,
        error: TransportError,
    },
}

pub struct Dispatcher<T: CodexTransport> {
    transport: T,
    generation: u64,
    next_request_id: u64,
    gate: MutationGate,
}

impl<T: CodexTransport> Dispatcher<T> {
    pub fn new(transport: T, generation: u64) -> Self {
        Self {
            transport,
            generation,
            next_request_id: 0,
            gate: MutationGate::new(generation),
        }
    }

    pub fn dispatch(&mut self, bound: BoundAction) -> DispatchResult {
        let action = bound.action.clone();
        let Some(target) = bound.target.as_ref() else {
            return DispatchResult::Rejected {
                action,
                error: TransportError::UnassignedTarget,
            };
        };
        self.next_request_id += 1;
        let identity = MutationIdentity {
            connection_generation: self.generation,
            request_id: self.next_request_id,
            method: method_for(&action).to_owned(),
            thread_id: target.thread_id.clone(),
            turn_id: target.turn_id.clone(),
            item_id: target.item_id.clone(),
            approval_id: target.approval_id.clone(),
        };
        let mutation = Mutation {
            identity: identity.clone(),
            action: action.clone(),
        };
        if let Err(error) = self.gate.accept(&mutation, &bound) {
            return DispatchResult::Rejected { action, error };
        }
        match self.transport.mutate(&mutation) {
            Ok(()) => DispatchResult::Applied(identity),
            Err(error) => DispatchResult::Rejected { action, error },
        }
    }

    #[cfg(test)]
    fn into_transport(self) -> T {
        self.transport
    }
}

fn method_for(action: &SemanticAction) -> &'static str {
    match action {
        SemanticAction::Activate => "thread/open",
        SemanticAction::ToggleFast => "thread/fast/toggle",
        SemanticAction::Approve => "turn/approval/accept",
        SemanticAction::Decline => "turn/approval/decline",
        SemanticAction::ContinueInNewChat => "thread/fork",
        SemanticAction::PushToTalk { active: true, .. } => "composer/ptt/start",
        SemanticAction::PushToTalk { active: false, .. } => "composer/ptt/stop",
        SemanticAction::Send => "turn/start",
        SemanticAction::Cardinal { .. } => "composer/cardinal",
        SemanticAction::SetReasoning(_) => "thread/reasoning/set",
        SemanticAction::Command(_) => "command/run",
        SemanticAction::Skill(_) => "skill/run",
    }
}

pub struct MutationGate {
    generation: u64,
    accepted: HashSet<MutationIdentity>,
    replay_order: VecDeque<MutationIdentity>,
}

impl MutationGate {
    pub fn new(generation: u64) -> Self {
        Self {
            generation,
            accepted: HashSet::new(),
            replay_order: VecDeque::new(),
        }
    }

    pub fn accept(
        &mut self,
        mutation: &Mutation,
        bound: &BoundAction,
    ) -> Result<(), TransportError> {
        let id = &mutation.identity;
        if id.connection_generation != self.generation {
            return Err(TransportError::StaleGeneration);
        }
        let Some(expected) = bound.target.as_ref() else {
            return Err(TransportError::UnassignedTarget);
        };
        if id.thread_id != expected.thread_id
            || id.turn_id != expected.turn_id
            || id.item_id != expected.item_id
            || id.approval_id != expected.approval_id
            || id.method != method_for(&mutation.action)
        {
            return Err(TransportError::ContextMismatch);
        }
        if !self.accepted.insert(id.clone()) {
            return Err(TransportError::DuplicateMutation);
        }
        self.replay_order.push_back(id.clone());
        if self.replay_order.len() > MAX_REPLAY_IDS
            && let Some(expired) = self.replay_order.pop_front()
        {
            self.accepted.remove(&expired);
        }
        Ok(())
    }

    #[cfg(test)]
    fn retained(&self) -> usize {
        self.accepted.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexEventKind {
    Snapshot {
        threads: Vec<ThreadRecord>,
        policy: SourcePolicy,
        custom_order: Vec<String>,
    },
    Upsert(ThreadRecord),
    Status {
        context: ThreadContext,
        status: ChatStatus,
        updated_ms: u64,
    },
    Remove {
        thread_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexEvent {
    pub connection_generation: u64,
    pub sequence: u64,
    pub kind: CodexEventKind,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AnalogGate {
    active: Option<DPad>,
}
impl AnalogGate {
    pub fn update(&mut self, x: u8, y: u8, dead_zone: u8, hysteresis: u8) -> Option<DPad> {
        let dx = x as i16 - 128;
        let dy = y as i16 - 128;
        let release = dead_zone.saturating_sub(hysteresis) as i16;
        if self.active.is_some() {
            if dx.abs().max(dy.abs()) <= release {
                self.active = None;
            }
            return None;
        }
        if dx.abs().max(dy.abs()) < dead_zone as i16 {
            return None;
        }
        // Exact diagonal ties are vertical, consistently.
        let direction = if dx.abs() > dy.abs() {
            if dx > 0 { DPad::Right } else { DPad::Left }
        } else if dy > 0 {
            DPad::Down
        } else {
            DPad::Up
        };
        self.active = Some(direction);
        Some(direction)
    }
}

pub struct CodexMicro {
    pub slots: [ChatSlot; SLOT_COUNT],
    threads: Vec<ThreadRecord>,
    pub selected: usize,
    pub reasoning: u8,
    policy: SourcePolicy,
    custom_order: Vec<String>,
    event_generation: u64,
    generation_started: bool,
    last_event_sequence: Option<u64>,
    last_slot_press: [Option<u64>; SLOT_COUNT],
    ptt_last_press: Option<u64>,
    ptt_context: Option<ThreadContext>,
    ptt_pressed: bool,
    ptt_latched: bool,
    prev: ButtonState,
    awaiting_neutral: bool,
    draining_modifier: bool,
    analog: AnalogGate,
    pub last_activity_ms: u64,
    pub transport_degraded: bool,
}

impl Default for CodexMicro {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| ChatSlot::default()),
            threads: Vec::new(),
            selected: 0,
            reasoning: 2,
            policy: SourcePolicy::Recent,
            custom_order: Vec::new(),
            event_generation: 0,
            generation_started: false,
            last_event_sequence: None,
            last_slot_press: [None; SLOT_COUNT],
            ptt_last_press: None,
            ptt_context: None,
            ptt_pressed: false,
            ptt_latched: false,
            prev: ButtonState::default(),
            awaiting_neutral: true,
            draining_modifier: false,
            analog: AnalogGate::default(),
            last_activity_ms: 0,
            transport_degraded: false,
        }
    }
}

impl CodexMicro {
    pub fn begin_generation(&mut self, generation: u64) -> Result<(), TransportError> {
        let expected = if self.generation_started {
            self.event_generation.saturating_add(1)
        } else {
            1
        };
        if generation != expected {
            return Err(TransportError::StaleGeneration);
        }
        self.event_generation = generation;
        self.generation_started = true;
        self.last_event_sequence = None;
        // A new transport connection must earn all authoritative state again.
        // Retaining old slots would let controller actions target a thread from
        // the previous connection while carrying the new generation number.
        self.threads.clear();
        self.slots = std::array::from_fn(|_| ChatSlot::default());
        self.selected = 0;
        self.ptt_context = None;
        self.transport_degraded = false;
        Ok(())
    }

    fn bind(&self, action: SemanticAction) -> BoundAction {
        BoundAction {
            action,
            target: self.selected_context().cloned(),
        }
    }

    pub fn configure_sources(&mut self, policy: SourcePolicy, custom_order: Vec<String>) {
        self.policy = policy;
        self.custom_order = custom_order;
        self.arrange();
    }

    pub fn selected_context(&self) -> Option<&ThreadContext> {
        self.slots[self.selected]
            .thread
            .as_ref()
            .map(|t| &t.context)
    }

    pub fn reduce(&mut self, event: CodexEvent, now_ms: u64) -> Result<(), TransportError> {
        if !self.generation_started || event.connection_generation != self.event_generation {
            return Err(TransportError::StaleGeneration);
        }
        if self
            .last_event_sequence
            .is_some_and(|s| event.sequence <= s)
        {
            return Err(TransportError::OutOfOrderEvent);
        }
        match event.kind {
            CodexEventKind::Snapshot {
                threads,
                policy,
                custom_order,
            } => {
                self.threads = threads;
                self.policy = policy;
                self.custom_order = custom_order;
            }
            CodexEventKind::Upsert(record) => {
                if let Some(old) = self
                    .threads
                    .iter_mut()
                    .find(|t| t.context.thread_id == record.context.thread_id)
                {
                    *old = record;
                } else {
                    self.threads.push(record);
                }
            }
            CodexEventKind::Status {
                context,
                status,
                updated_ms,
            } => {
                let Some(old) = self
                    .threads
                    .iter_mut()
                    .find(|t| t.context.thread_id == context.thread_id)
                else {
                    return Err(TransportError::ContextMismatch);
                };
                if old.context != context {
                    return Err(TransportError::ContextMismatch);
                }
                old.status = status;
                old.updated_ms = updated_ms;
            }
            CodexEventKind::Remove { thread_id } => {
                self.threads.retain(|t| t.context.thread_id != thread_id)
            }
        }
        self.last_event_sequence = Some(event.sequence);
        self.arrange();
        self.last_activity_ms = now_ms;
        Ok(())
    }

    fn arrange(&mut self) {
        let selected_id = self.selected_context().map(|c| c.thread_id.clone());
        match self.policy {
            SourcePolicy::Recent => self
                .threads
                .sort_by_key(|t| std::cmp::Reverse(t.updated_ms)),
            SourcePolicy::Pinned => self
                .threads
                .sort_by_key(|t| (std::cmp::Reverse(t.pinned), std::cmp::Reverse(t.updated_ms))),
            SourcePolicy::Priority => self.threads.sort_by_key(|t| {
                (
                    std::cmp::Reverse(t.priority),
                    std::cmp::Reverse(t.updated_ms),
                )
            }),
            SourcePolicy::Custom => {
                let order = &self.custom_order;
                self.threads.sort_by_key(|t| {
                    (
                        order
                            .iter()
                            .position(|id| id == &t.context.thread_id)
                            .unwrap_or(usize::MAX),
                        std::cmp::Reverse(t.updated_ms),
                    )
                });
            }
        }
        self.threads.truncate(MAX_THREADS);
        for (index, slot) in self.slots.iter_mut().enumerate() {
            slot.thread = self.threads.get(index).cloned();
        }
        if let Some(id) = selected_id
            && let Some(index) = self
                .slots
                .iter()
                .position(|s| s.thread.as_ref().is_some_and(|t| t.context.thread_id == id))
        {
            self.selected = index;
        }
        self.selected = self.selected.min(SLOT_COUNT - 1);
    }

    pub fn select(&mut self, slot: usize, now_ms: u64) -> Option<BoundAction> {
        if slot >= SLOT_COUNT {
            return None;
        }
        let activate = self.last_slot_press[slot]
            .is_some_and(|t| now_ms >= t && now_ms - t <= DOUBLE_PRESS_MS);
        self.selected = slot;
        self.last_slot_press[slot] = if activate { None } else { Some(now_ms) };
        self.last_activity_ms = now_ms;
        activate.then(|| self.bind(SemanticAction::Activate))
    }

    fn ptt_press(&mut self, now_ms: u64) -> Vec<BoundAction> {
        if self.ptt_latched {
            self.ptt_latched = false;
            self.ptt_pressed = false;
            self.ptt_last_press = None;
            let target = self.ptt_context.take();
            return vec![BoundAction {
                action: SemanticAction::PushToTalk {
                    active: false,
                    latched: false,
                },
                target,
            }];
        }
        let double = self
            .ptt_last_press
            .is_some_and(|t| now_ms >= t && now_ms - t <= DOUBLE_PRESS_MS);
        self.ptt_pressed = true;
        if double {
            self.ptt_latched = true;
            self.ptt_last_press = None;
            vec![BoundAction {
                action: SemanticAction::PushToTalk {
                    active: true,
                    latched: true,
                },
                target: self.ptt_context.clone(),
            }]
        } else {
            self.ptt_last_press = Some(now_ms);
            self.ptt_context = self.selected_context().cloned();
            vec![BoundAction {
                action: SemanticAction::PushToTalk {
                    active: true,
                    latched: false,
                },
                target: self.ptt_context.clone(),
            }]
        }
    }

    fn ptt_release(&mut self) -> Vec<BoundAction> {
        let was_pressed = self.ptt_pressed;
        self.ptt_pressed = false;
        if self.ptt_latched || !was_pressed {
            Vec::new()
        } else {
            vec![BoundAction {
                action: SemanticAction::PushToTalk {
                    active: false,
                    latched: false,
                },
                target: self.ptt_context.clone(),
            }]
        }
    }

    pub fn reconnect(&mut self) -> Vec<BoundAction> {
        let stop = (self.ptt_pressed || self.ptt_latched)
            .then_some(BoundAction {
                action: SemanticAction::PushToTalk {
                    active: false,
                    latched: false,
                },
                target: self.ptt_context.clone(),
            })
            .into_iter()
            .collect();
        self.prev = ButtonState::default();
        self.awaiting_neutral = true;
        self.draining_modifier = false;
        self.ptt_pressed = false;
        self.ptt_latched = false;
        self.ptt_last_press = None;
        self.ptt_context = None;
        self.analog = AnalogGate::default();
        stop
    }

    pub fn mark_dispatch(&mut self, result: &DispatchResult, now_ms: u64) {
        self.transport_degraded = matches!(result, DispatchResult::Rejected { .. });
        self.last_activity_ms = now_ms;
    }

    pub fn update_input(
        &mut self,
        input: &UnifiedInput,
        now_ms: u64,
        cfg: &CodexMicroConfig,
    ) -> (Vec<BoundAction>, bool) {
        if !cfg.prototype_active() {
            self.prev = input.buttons;
            return (Vec::new(), false);
        }
        let b = input.buttons;
        let neutral = !b.ps
            && !b.cross
            && !b.circle
            && !b.square
            && !b.triangle
            && !b.l1
            && !b.r1
            && !b.l2
            && !b.r2
            && !b.options
            && !b.share
            && !b.l3
            && !b.r3
            && !b.touchpad
            && b.dpad == DPad::Neutral
            && (input.right_stick.0 as i16 - 128)
                .abs()
                .max((input.right_stick.1 as i16 - 128).abs())
                < cfg.analog_dead_zone as i16;
        if self.awaiting_neutral {
            self.prev = b;
            if neutral {
                self.awaiting_neutral = false;
            }
            return (Vec::new(), true);
        }
        let modifier = b.ps;
        if modifier {
            self.draining_modifier = true;
        }
        let consumed = modifier || self.prev.ps || self.draining_modifier;
        let mut out = Vec::new();
        if consumed {
            self.last_activity_ms = now_ms;
            let pressed = |n: bool, p: bool| modifier && n && !p;
            // Selection is reduced first, so every simultaneous mutation is
            // immutably bound to the newly selected slot.
            if pressed(b.l1, self.prev.l1)
                && let Some(a) = self.select(self.selected.saturating_sub(1), now_ms)
            {
                out.push(a);
            }
            if pressed(b.r1, self.prev.r1)
                && let Some(a) = self.select((self.selected + 1).min(SLOT_COUNT - 1), now_ms)
            {
                out.push(a);
            }
            if pressed(b.touchpad, self.prev.touchpad)
                && let Some(a) = self.select(self.selected, now_ms)
            {
                out.push(a);
            }
            if pressed(b.cross, self.prev.cross) {
                out.push(self.bind(SemanticAction::Approve));
            }
            if pressed(b.circle, self.prev.circle) {
                out.push(self.bind(SemanticAction::Decline));
            }
            if pressed(b.triangle, self.prev.triangle) {
                out.push(self.bind(SemanticAction::Send));
            }
            if pressed(b.square, self.prev.square) {
                out.push(self.bind(SemanticAction::ToggleFast));
            }
            if pressed(b.options, self.prev.options) {
                out.push(self.bind(SemanticAction::ContinueInNewChat));
            }
            if pressed(b.l2, self.prev.l2) {
                out.extend(self.ptt_press(now_ms));
            }
            if !b.l2 && self.prev.l2 {
                out.extend(self.ptt_release());
            }
            if pressed(
                matches!(b.dpad, DPad::Up),
                matches!(self.prev.dpad, DPad::Up),
            ) {
                self.reasoning = (self.reasoning + 1).min(4);
                out.push(self.bind(SemanticAction::SetReasoning(self.reasoning)));
            }
            if pressed(
                matches!(b.dpad, DPad::Down),
                matches!(self.prev.dpad, DPad::Down),
            ) {
                self.reasoning = self.reasoning.saturating_sub(1);
                out.push(self.bind(SemanticAction::SetReasoning(self.reasoning)));
            }
            if pressed(b.l3, self.prev.l3)
                && let Some((_, command)) = cfg.commands.iter().min_by_key(|(name, _)| *name)
            {
                out.push(self.bind(SemanticAction::Command(command.clone())));
            }
            if pressed(b.r3, self.prev.r3)
                && let Some((_, skill)) = cfg.skills.iter().min_by_key(|(name, _)| *name)
            {
                out.push(self.bind(SemanticAction::Skill(skill.clone())));
            }
            if modifier
                && let Some(direction) = self.analog.update(
                    input.right_stick.0,
                    input.right_stick.1,
                    cfg.analog_dead_zone,
                    cfg.analog_hysteresis,
                )
            {
                out.push(self.bind(SemanticAction::Cardinal { direction }));
            }
        }
        self.prev = b;
        if !modifier && neutral {
            self.draining_modifier = false;
        }
        (out, consumed)
    }

    pub fn rgb(&self, now_ms: u64, cfg: &CodexMicroConfig) -> [u8; 3] {
        if now_ms.saturating_sub(self.last_activity_ms)
            >= cfg.inactivity_seconds.saturating_mul(1000)
        {
            return [0, 0, 0];
        }
        let status = if self.transport_degraded {
            ChatStatus::Error
        } else {
            self.slots[self.selected]
                .thread
                .as_ref()
                .map_or(ChatStatus::Unassigned, |t| t.status)
        };
        let pulse_percent = if (now_ms / 500).is_multiple_of(2) {
            100
        } else {
            55
        };
        let scale = u16::from(cfg.brightness.min(100)) * pulse_percent / 100;
        status
            .color()
            .map(|channel| ((u16::from(channel) * scale + 50) / 100) as u8)
    }
}

pub fn compose_rgb(
    state: &CodexMicro,
    cfg: &CodexMicroConfig,
    legacy: [u8; 3],
    now_ms: u64,
) -> [u8; 3] {
    if cfg.prototype_active() {
        state.rgb(now_ms, cfg)
    } else {
        legacy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingTransport(Vec<Mutation>);
    impl CodexTransport for RecordingTransport {
        fn mutate(&mut self, mutation: &Mutation) -> Result<(), TransportError> {
            self.0.push(mutation.clone());
            Ok(())
        }
    }

    fn context(id: &str) -> ThreadContext {
        ThreadContext {
            thread_id: id.into(),
            turn_id: Some("turn".into()),
            item_id: Some("item".into()),
            approval_id: Some("approval".into()),
        }
    }
    fn record(id: &str, updated_ms: u64, status: ChatStatus) -> ThreadRecord {
        ThreadRecord {
            context: context(id),
            status,
            updated_ms,
            pinned: false,
            priority: 0,
        }
    }
    fn enabled() -> CodexMicroConfig {
        CodexMicroConfig {
            enabled: true,
            demo_mode: true,
            ..Default::default()
        }
    }
    fn neutralize(state: &mut CodexMicro, cfg: &CodexMicroConfig) {
        state.update_input(&UnifiedInput::default(), 0, cfg);
    }
    fn slot_delta(delta: u64) -> Option<SemanticAction> {
        let mut s = CodexMicro::default();
        s.select(0, 1);
        s.select(0, 1 + delta).map(|bound| bound.action)
    }

    #[test]
    fn slot_boundary_is_inclusive_and_zero_is_valid_first_press() {
        assert_eq!(slot_delta(349), Some(SemanticAction::Activate));
        assert_eq!(slot_delta(350), Some(SemanticAction::Activate));
        assert_eq!(slot_delta(351), None);
        let mut s = CodexMicro::default();
        assert_eq!(s.select(0, 0), None);
        assert_eq!(
            s.select(0, 1).map(|bound| bound.action),
            Some(SemanticAction::Activate)
        );
    }

    fn ptt_second_press(delta: u64) -> Vec<SemanticAction> {
        let cfg = enabled();
        let mut s = CodexMicro::default();
        neutralize(&mut s, &cfg);
        let mut i = UnifiedInput::default();
        i.buttons.ps = true;
        i.buttons.l2 = true;
        s.update_input(&i, 1, &cfg);
        i.buttons.l2 = false;
        s.update_input(&i, 2, &cfg);
        i.buttons.l2 = true;
        s.update_input(&i, 1 + delta, &cfg)
            .0
            .into_iter()
            .map(|bound| bound.action)
            .collect()
    }
    #[test]
    fn ptt_double_boundary_and_long_hold() {
        for d in [349, 350] {
            assert_eq!(
                ptt_second_press(d),
                vec![SemanticAction::PushToTalk {
                    active: true,
                    latched: true
                }]
            );
        }
        assert_eq!(
            ptt_second_press(351),
            vec![SemanticAction::PushToTalk {
                active: true,
                latched: false
            }]
        );
        let mut zero = CodexMicro::default();
        assert_eq!(
            zero.ptt_press(0),
            vec![SemanticAction::PushToTalk {
                active: true,
                latched: false
            }]
        );
        let cfg = enabled();
        let mut s = CodexMicro::default();
        neutralize(&mut s, &cfg);
        let mut i = UnifiedInput::default();
        i.buttons.ps = true;
        i.buttons.l2 = true;
        assert_eq!(
            s.update_input(&i, 10, &cfg).0,
            vec![SemanticAction::PushToTalk {
                active: true,
                latched: false
            }]
        );
        i.buttons.l2 = false;
        assert_eq!(
            s.update_input(&i, 1000, &cfg).0,
            vec![SemanticAction::PushToTalk {
                active: false,
                latched: false
            }]
        );
    }
    #[test]
    fn ptt_latch_next_press_stops_and_reconnect_stops() {
        let cfg = enabled();
        let mut s = CodexMicro::default();
        neutralize(&mut s, &cfg);
        let mut i = UnifiedInput::default();
        i.buttons.ps = true;
        i.buttons.l2 = true;
        s.update_input(&i, 10, &cfg);
        i.buttons.l2 = false;
        s.update_input(&i, 20, &cfg);
        i.buttons.l2 = true;
        s.update_input(&i, 100, &cfg);
        i.buttons.l2 = false;
        s.update_input(&i, 110, &cfg);
        i.buttons.l2 = true;
        assert_eq!(
            s.update_input(&i, 500, &cfg).0,
            vec![SemanticAction::PushToTalk {
                active: false,
                latched: false
            }]
        );
        i.buttons.l2 = false;
        assert!(s.update_input(&i, 510, &cfg).0.is_empty());
        assert_eq!(s.reconnect(), Vec::<SemanticAction>::new());
        s.ptt_pressed = true;
        assert_eq!(
            s.reconnect(),
            vec![SemanticAction::PushToTalk {
                active: false,
                latched: false
            }]
        );
    }

    #[test]
    fn event_reducer_orders_guards_and_populates() {
        let mut s = CodexMicro::default();
        s.begin_generation(1).unwrap();
        s.begin_generation(2).unwrap();
        let mut a = record("a", 1, ChatStatus::Idle);
        let mut b = record("b", 2, ChatStatus::Thinking);
        b.pinned = true;
        a.priority = 9;
        s.reduce(
            CodexEvent {
                connection_generation: 2,
                sequence: 1,
                kind: CodexEventKind::Snapshot {
                    threads: vec![a.clone(), b.clone()],
                    policy: SourcePolicy::Recent,
                    custom_order: vec![],
                },
            },
            10,
        )
        .unwrap();
        assert_eq!(s.slots[0].thread.as_ref().unwrap().context.thread_id, "b");
        assert_eq!(
            s.reduce(
                CodexEvent {
                    connection_generation: 2,
                    sequence: 1,
                    kind: CodexEventKind::Remove {
                        thread_id: "a".into()
                    }
                },
                11
            ),
            Err(TransportError::OutOfOrderEvent)
        );
        assert_eq!(
            s.reduce(
                CodexEvent {
                    connection_generation: 1,
                    sequence: 9,
                    kind: CodexEventKind::Remove {
                        thread_id: "a".into()
                    }
                },
                11
            ),
            Err(TransportError::StaleGeneration)
        );
        s.reduce(
            CodexEvent {
                connection_generation: 2,
                sequence: 2,
                kind: CodexEventKind::Snapshot {
                    threads: vec![a.clone(), b.clone()],
                    policy: SourcePolicy::Pinned,
                    custom_order: vec![],
                },
            },
            12,
        )
        .unwrap();
        assert_eq!(s.slots[0].thread.as_ref().unwrap().context.thread_id, "b");
        s.reduce(
            CodexEvent {
                connection_generation: 2,
                sequence: 3,
                kind: CodexEventKind::Snapshot {
                    threads: vec![b.clone(), a.clone()],
                    policy: SourcePolicy::Priority,
                    custom_order: vec![],
                },
            },
            13,
        )
        .unwrap();
        assert_eq!(s.slots[0].thread.as_ref().unwrap().context.thread_id, "a");
        s.reduce(
            CodexEvent {
                connection_generation: 2,
                sequence: 4,
                kind: CodexEventKind::Snapshot {
                    threads: vec![a, b],
                    policy: SourcePolicy::Custom,
                    custom_order: vec!["b".into(), "a".into()],
                },
            },
            14,
        )
        .unwrap();
        assert_eq!(s.slots[0].thread.as_ref().unwrap().context.thread_id, "b");
    }
    #[test]
    fn status_requires_exact_context_and_wakes() {
        let mut s = CodexMicro::default();
        s.begin_generation(1).unwrap();
        s.reduce(
            CodexEvent {
                connection_generation: 1,
                sequence: 1,
                kind: CodexEventKind::Upsert(record("a", 1, ChatStatus::Idle)),
            },
            1,
        )
        .unwrap();
        for field in 0..3 {
            let mut wrong = context("a");
            match field {
                0 => wrong.turn_id = Some("wrong".into()),
                1 => wrong.item_id = Some("wrong".into()),
                _ => wrong.approval_id = Some("wrong".into()),
            }
            assert_eq!(
                s.reduce(
                    CodexEvent {
                        connection_generation: 1,
                        sequence: 2,
                        kind: CodexEventKind::Status {
                            context: wrong,
                            status: ChatStatus::Error,
                            updated_ms: 2
                        }
                    },
                    200
                ),
                Err(TransportError::ContextMismatch)
            );
        }
        s.reduce(
            CodexEvent {
                connection_generation: 1,
                sequence: 2,
                kind: CodexEventKind::Status {
                    context: context("a"),
                    status: ChatStatus::CompleteUnread,
                    updated_ms: 2,
                },
            },
            200,
        )
        .unwrap();
        assert_eq!(s.last_activity_ms, 200);
    }

    #[test]
    fn mutation_identity_rejects_every_mismatch_and_duplicate() {
        let ctx = context("a");
        let bound = BoundAction {
            action: SemanticAction::Approve,
            target: Some(ctx.clone()),
        };
        let mut d = Dispatcher::new(RecordingTransport::default(), 7);
        assert!(matches!(
            d.dispatch(bound.clone()),
            DispatchResult::Applied(_)
        ));
        assert!(matches!(
            d.dispatch(bound.clone()),
            DispatchResult::Applied(_)
        ));
        let recorded = d.into_transport().0;
        assert_eq!(recorded[0].identity.request_id, 1);
        assert_eq!(recorded[1].identity.request_id, 2);
        assert_eq!(recorded[0].identity.thread_id, "a");
        let id = MutationIdentity {
            connection_generation: 7,
            request_id: 1,
            method: "turn/approval/accept".into(),
            thread_id: "a".into(),
            turn_id: ctx.turn_id.clone(),
            item_id: ctx.item_id.clone(),
            approval_id: ctx.approval_id.clone(),
        };
        let m = Mutation {
            identity: id.clone(),
            action: SemanticAction::Approve,
        };
        let mut gate = MutationGate::new(7);
        assert_eq!(gate.accept(&m, &bound), Ok(()));
        assert_eq!(
            gate.accept(&m, &bound),
            Err(TransportError::DuplicateMutation)
        );
        let mut stale = m.clone();
        stale.identity.connection_generation = 6;
        assert_eq!(
            MutationGate::new(7).accept(&stale, &bound),
            Err(TransportError::StaleGeneration)
        );
        for mutate in [
            |id: &mut MutationIdentity| id.method = "wrong".into(),
            |id: &mut MutationIdentity| id.thread_id = "wrong".into(),
            |id: &mut MutationIdentity| id.turn_id = Some("wrong".into()),
            |id: &mut MutationIdentity| id.item_id = Some("wrong".into()),
            |id: &mut MutationIdentity| id.approval_id = Some("wrong".into()),
        ] {
            let mut wrong = m.clone();
            mutate(&mut wrong.identity);
            assert_eq!(
                MutationGate::new(7).accept(&wrong, &bound),
                Err(TransportError::ContextMismatch)
            );
        }
        let mut bounded = MutationGate::new(7);
        for request_id in 1..=(MAX_REPLAY_IDS as u64 + 1) {
            let mut next = m.clone();
            next.identity.request_id = request_id;
            bounded.accept(&next, &bound).unwrap();
        }
        assert_eq!(bounded.retained(), MAX_REPLAY_IDS);
    }

    #[test]
    fn analog_requires_neutral_and_ties_are_vertical() {
        let mut g = AnalogGate::default();
        assert_eq!(g.update(250, 128, 40, 10), Some(DPad::Right));
        assert_eq!(g.update(0, 128, 40, 10), None);
        assert_eq!(g.update(128, 128, 40, 10), None);
        assert_eq!(g.update(0, 128, 40, 10), Some(DPad::Left));
        let mut boundary = AnalogGate::default();
        assert_eq!(boundary.update(200, 128, 40, 10), Some(DPad::Right));
        assert_eq!(boundary.update(159, 128, 40, 10), None); // 31: still active
        assert_eq!(boundary.update(158, 128, 40, 10), None); // 30: exact release
        assert_eq!(boundary.update(80, 128, 40, 10), Some(DPad::Left));
        let mut tie = AnalogGate::default();
        assert_eq!(tie.update(200, 200, 40, 10), Some(DPad::Down));
        let mut tie2 = AnalogGate::default();
        assert_eq!(tie2.update(56, 56, 40, 10), Some(DPad::Up));
    }
    #[test]
    fn reconnect_requires_neutral_and_commands_reasoning_are_wired() {
        let mut cfg = enabled();
        cfg.commands.insert("a".into(), "review".into());
        cfg.skills.insert("a".into(), "test".into());
        let mut s = CodexMicro::default();
        let mut i = UnifiedInput::default();
        i.buttons.ps = true;
        i.buttons.cross = true;
        assert!(s.update_input(&i, 1, &cfg).0.is_empty());
        i = UnifiedInput::default();
        s.update_input(&i, 2, &cfg);
        i.buttons.ps = true;
        i.buttons.l3 = true;
        assert_eq!(
            s.update_input(&i, 3, &cfg).0,
            vec![SemanticAction::Command("review".into())]
        );
        i.buttons.l3 = false;
        i.buttons.r3 = true;
        assert_eq!(
            s.update_input(&i, 4, &cfg).0,
            vec![SemanticAction::Skill("test".into())]
        );
        i.buttons.r3 = false;
        i.buttons.dpad = DPad::Up;
        assert_eq!(
            s.update_input(&i, 5, &cfg).0,
            vec![SemanticAction::SetReasoning(3)]
        );
    }
    #[test]
    fn enabled_without_demo_mode_preserves_legacy_controls() {
        let cfg = CodexMicroConfig {
            enabled: true,
            demo_mode: false,
            ..Default::default()
        };
        let mut state = CodexMicro::default();
        let mut input = UnifiedInput::default();
        input.buttons.ps = true;
        input.buttons.cross = true;
        let (actions, consumed) = state.update_input(&input, 1, &cfg);
        assert!(actions.is_empty());
        assert!(!consumed);
        assert_eq!(compose_rgb(&state, &cfg, [9, 8, 7], 1), [9, 8, 7]);
    }

    #[test]
    fn lifecycle_resets_sequence_and_bounds_threads() {
        let mut state = CodexMicro::default();
        state.begin_generation(1).unwrap();
        state
            .reduce(
                CodexEvent {
                    connection_generation: 1,
                    sequence: u64::MAX - 1,
                    kind: CodexEventKind::Snapshot {
                        threads: (0..100)
                            .map(|n| record(&format!("t{n}"), n, ChatStatus::Idle))
                            .collect(),
                        policy: SourcePolicy::Recent,
                        custom_order: vec![],
                    },
                },
                1,
            )
            .unwrap();
        assert_eq!(state.threads.len(), MAX_THREADS);
        assert_eq!(
            state.begin_generation(3),
            Err(TransportError::StaleGeneration)
        );
        state.begin_generation(2).unwrap();
        assert!(state.threads.is_empty());
        assert!(state.slots.iter().all(|slot| slot.thread.is_none()));
        assert!(state.selected_context().is_none());
        state
            .reduce(
                CodexEvent {
                    connection_generation: 2,
                    sequence: 1,
                    kind: CodexEventKind::Remove {
                        thread_id: "t99".into(),
                    },
                },
                2,
            )
            .unwrap();
    }

    #[test]
    fn simultaneous_selection_and_ptt_release_keep_creation_targets() {
        let cfg = enabled();
        let mut state = CodexMicro::default();
        state.begin_generation(1).unwrap();
        state
            .reduce(
                CodexEvent {
                    connection_generation: 1,
                    sequence: 1,
                    kind: CodexEventKind::Snapshot {
                        threads: vec![
                            record("a", 2, ChatStatus::Idle),
                            record("b", 1, ChatStatus::Idle),
                        ],
                        policy: SourcePolicy::Custom,
                        custom_order: vec!["a".into(), "b".into()],
                    },
                },
                0,
            )
            .unwrap();
        neutralize(&mut state, &cfg);
        let mut input = UnifiedInput::default();
        input.buttons.ps = true;
        input.buttons.r1 = true;
        input.buttons.cross = true;
        let actions = state.update_input(&input, 10, &cfg).0;
        let approve = actions
            .iter()
            .find(|a| a.action == SemanticAction::Approve)
            .unwrap();
        assert_eq!(approve.target.as_ref().unwrap().thread_id, "b");
        input.buttons.r1 = false;
        input.buttons.cross = false;
        input.buttons.l2 = true;
        let start = state.update_input(&input, 20, &cfg).0;
        assert_eq!(start[0].target.as_ref().unwrap().thread_id, "b");
        state.selected = 0;
        input.buttons.l2 = false;
        let stop = state.update_input(&input, 30, &cfg).0;
        assert_eq!(stop[0].target.as_ref().unwrap().thread_id, "b");
    }
    #[test]
    fn colors_selected_pulse_brightness_sleep_and_event_wake() {
        assert_eq!(ChatStatus::Idle.color(), [255, 255, 255]);
        assert_eq!(ChatStatus::CompleteUnread.color(), [34, 197, 94]);
        assert_eq!(ChatStatus::RequiresInput.color(), [245, 158, 11]);
        let mut s = CodexMicro::default();
        s.begin_generation(1).unwrap();
        s.reduce(
            CodexEvent {
                connection_generation: 1,
                sequence: 1,
                kind: CodexEventKind::Snapshot {
                    threads: vec![
                        record("selected", 1, ChatStatus::Idle),
                        record("error", 2, ChatStatus::Error),
                    ],
                    policy: SourcePolicy::Custom,
                    custom_order: vec!["selected".into()],
                },
            },
            0,
        )
        .unwrap();
        let mut c = enabled();
        c.brightness = 0;
        assert_eq!(s.rgb(0, &c), [0, 0, 0]);
        c.brightness = 100;
        assert_eq!(s.rgb(0, &c), [255, 255, 255]);
        assert_eq!(s.rgb(500, &c), [140, 140, 140]);
        c.brightness = 255;
        assert_eq!(s.rgb(0, &c), [255, 255, 255]);
        c.inactivity_seconds = 1;
        assert_eq!(s.rgb(1000, &c), [0, 0, 0]);
        s.reduce(
            CodexEvent {
                connection_generation: 1,
                sequence: 2,
                kind: CodexEventKind::Status {
                    context: context("selected"),
                    status: ChatStatus::Thinking,
                    updated_ms: 2,
                },
            },
            1001,
        )
        .unwrap();
        assert_ne!(s.rgb(1001, &c), [0, 0, 0]);
        s.transport_degraded = true;
        assert_eq!(s.rgb(1001, &c), ChatStatus::Error.color());
    }
    #[test]
    fn opt_out_preserves_legacy_rgb() {
        let s = CodexMicro::default();
        assert_eq!(
            compose_rgb(&s, &CodexMicroConfig::default(), [1, 2, 3], 99),
            [1, 2, 3]
        );
    }
}
