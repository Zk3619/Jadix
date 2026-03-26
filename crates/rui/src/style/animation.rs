use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::core::reactivity::{use_signal, Signal};

thread_local! {
    static ANIMATIONS: RefCell<Vec<Weak<RefCell<SpringState>>>> = RefCell::new(Vec::new());
}

pub struct SpringState {
    pub target: f32,
    pub current: f32,
    pub velocity: f32,
    pub stiffness: f32,
    pub damping: f32,
    pub signal: Signal<f32>,
}

#[derive(Clone)]
pub struct Spring(Rc<RefCell<SpringState>>);

impl Spring {
    pub fn new(initial: f32) -> Self {
        let state = Rc::new(RefCell::new(SpringState {
            target: initial,
            current: initial,
            velocity: 0.0,
            stiffness: 170.0,
            damping: 20.0,
            signal: Signal::new(initial),
        }));

        ANIMATIONS.with(|anim| {
            anim.borrow_mut().push(Rc::downgrade(&state));
        });

        Self(state)
    }

    pub fn set(&self, target: f32) {
        let mut state = self.0.borrow_mut();
        if (state.target - target).abs() < 0.0001 {
            return;
        }
        state.target = target;
        // Keep target updates from fan-out rebuilding effects synchronously.
        state.signal.set_silent(state.current);
    }

    pub fn get(&self) -> f32 {
        self.0.borrow().signal.get()
    }

    pub fn peek(&self) -> f32 {
        self.0.borrow().signal.peek()
    }
}

pub fn use_spring(id_str: &str, initial: f32) -> Spring {
    let spring_sig = use_signal(id_str, move || Spring::new(initial));
    // Peek: the stored `Spring` is stable; subscribing the root `create_effect` here would be
    // pointless and risks accidental coupling if the wrapper signal were ever updated.
    spring_sig.peek()
}

pub fn tick_animations(dt: f32) -> bool {
    const SPRING_SETTLE_POS_EPS: f32 = 0.01;
    const SPRING_SETTLE_VEL_EPS: f32 = 0.01;
    let mut any_running = false;
    let mut signals_to_update = Vec::new();

    ANIMATIONS.with(|anim| {
        let mut list = anim.borrow_mut();
        let mut i = 0;
        while i < list.len() {
            if let Some(spring) = list[i].upgrade() {
                let mut s = spring.borrow_mut();

                let displacement = s.current - s.target;
                let spring_force = -s.stiffness * displacement;
                let damping_force = -s.damping * s.velocity;
                let acceleration = spring_force + damping_force;

                s.velocity += acceleration * dt;
                s.current += s.velocity * dt;

                if displacement.abs() < SPRING_SETTLE_POS_EPS
                    && s.velocity.abs() < SPRING_SETTLE_VEL_EPS
                {
                    s.current = s.target;
                    s.velocity = 0.0;
                } else {
                    any_running = true;
                }

                signals_to_update.push((s.signal.clone(), s.current));
                i += 1;
            } else {
                list.remove(i);
            }
        }
    });

    for (sig, val) in signals_to_update {
        sig.set(val);
    }

    any_running
}
