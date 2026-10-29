//! Instructions per span (feature `trace`; docs/performance.md, "Map
//! changes that slowed the tick"): a tracing layer that reads the
//! entering thread's hardware counters (`counters::ThreadCounters`) at
//! each span's enter and exit and sums every span's own instructions
//! (its children's taken off) by label (`system: name="..."`, as
//! `tracesum` names them). Load-independent where wall-clock traces are
//! not, so two windows of a headless run can be compared system by
//! system: `SpanCosts::install()` on the thread that runs the schedules
//! before the app is built (a system's span is made with the system, and
//! one made with no subscriber stays off), `take()` after each window,
//! `rises(&before, &after, n)`.
//!
//! Known shortcuts: only spans entered on the installing thread are
//! counted (the single-threaded executors run every system there; work a
//! system hands to the task pools counts in no span); each enter and exit
//! is a counter read (a syscall), so a window costs more wall time.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    thread::ThreadId,
};

use bevy::log::{
    tracing::{self, Subscriber, field, span},
    tracing_subscriber::{Layer, layer::Context, prelude::*, registry::LookupSpan},
};

use super::ThreadCounters;

/// Instructions and calls per span label.
#[derive(Clone, Debug, Default)]
pub struct Costs(pub BTreeMap<String, (u64, u64)>);

#[derive(Default)]
struct State {
    counters: Option<ThreadCounters>,
    /// Open spans: label, instructions at entry, instructions of children.
    stack: Vec<(String, u64, u64)>,
    costs: Costs,
}

fn instructions(s: &mut State) -> Option<u64> {
    s.counters.get_or_insert_with(ThreadCounters::current).read().hw.map(|h| h.instructions)
}

struct Label(String);

/// The layer: the installing thread's open spans and sums (spans on
/// other threads are left out).
struct CostLayer {
    state: Arc<Mutex<State>>,
    owner: ThreadId,
}

impl CostLayer {
    fn with(&self, f: impl FnOnce(&mut State)) {
        if std::thread::current().id() == self.owner
            && let Ok(mut s) = self.state.lock()
        {
            f(&mut s);
        }
    }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for CostLayer {
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        struct Fields(String);
        impl field::Visit for Fields {
            fn record_debug(&mut self, f: &field::Field, v: &dyn std::fmt::Debug) {
                if !self.0.is_empty() {
                    self.0.push(' ');
                }
                self.0 += &format!("{}={v:?}", f.name());
            }
        }
        let mut fields = Fields(String::new());
        attrs.record(&mut fields);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut()
                .insert(Label(format!("{}: {}", attrs.metadata().name(), fields.0)));
        }
    }

    fn on_enter(&self, id: &span::Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let label = span.extensions().get::<Label>().map(|l| l.0.clone()).unwrap_or_default();
        self.with(|s| {
            if let Some(at) = instructions(s) {
                s.stack.push((label, at, 0));
            }
        });
    }

    fn on_exit(&self, _id: &span::Id, _ctx: Context<'_, S>) {
        self.with(|s| {
            let Some(now) = instructions(s) else { return };
            let Some((label, at, children)) = s.stack.pop() else { return };
            let total = now.saturating_sub(at);
            if let Some(parent) = s.stack.last_mut() {
                parent.2 += total;
            }
            let e = s.costs.0.entry(label).or_default();
            e.0 += total.saturating_sub(children);
            e.1 += 1;
        });
    }
}

/// The installed layer's handle; dropping it uninstalls it.
pub struct SpanCosts {
    state: Arc<Mutex<State>>,
    _guard: tracing::subscriber::DefaultGuard,
}

impl SpanCosts {
    /// Counts the spans entered on this thread from now on; None without
    /// hardware counters.
    pub fn install() -> Option<Self> {
        let state = Arc::new(Mutex::new(State::default()));
        {
            let mut s = state.lock().ok()?;
            // The first read opens the counters.
            instructions(&mut s).or_else(|| instructions(&mut s))?;
        }
        let layer = CostLayer {
            state: state.clone(),
            owner: std::thread::current().id(),
        };
        let subscriber = bevy::log::tracing_subscriber::registry().with(layer);
        Some(Self {
            state,
            _guard: tracing::subscriber::set_default(subscriber),
        })
    }

    /// The costs since the last `take` (or the install).
    pub fn take(&self) -> Costs {
        self.state.lock().map(|mut s| std::mem::take(&mut s.costs)).unwrap_or_default()
    }

    /// The installing thread's instructions so far, from the layer's own
    /// counters (a second counter group on the thread may not get the
    /// hardware's counters while this one has them).
    pub fn instructions(&self) -> Option<u64> {
        self.state.lock().ok().and_then(|mut s| instructions(&mut s))
    }
}

/// The `n` spans whose instructions per call rose most from `before` to
/// `after`, each `(label, before per call, after per call, after calls)`.
pub fn rises(before: &Costs, after: &Costs, n: usize) -> Vec<(String, f64, f64, u64)> {
    let per = |c: Option<&(u64, u64)>| c.map_or(0.0, |&(i, n)| if n == 0 { 0.0 } else { i as f64 / n as f64 });
    let mut rows: Vec<_> = after
        .0
        .iter()
        .map(|(k, a)| (k.clone(), per(before.0.get(k)), per(Some(a)), a.1))
        .collect();
    rows.sort_by(|a, b| ((b.2 - b.1) * b.3 as f64).total_cmp(&((a.2 - a.1) * a.3 as f64)));
    rows.truncate(n);
    rows
}
