//! Every patch pre-rendered to sample buffers (all variations), so playback only reads memory.

#[cfg(not(target_os = "emscripten"))]
use std::thread;

use super::patch::{Patch, render_patch, variation_seed};

/// Seed the game renders its bank with (variations are deterministic per seed).
pub const DEFAULT_BANK_SEED: u64 = 0xD111_6A4E;

/// Rendered variations of every patch, indexed by [`Patch::index`].
#[derive(Debug, Default)]
pub struct SoundBank {
    patches: Vec<Vec<Box<[f32]>>>,
}

impl SoundBank {
    /// Renders every patch's variations from `seed`, spread over the available cores.
    #[must_use]
    pub fn render(seed: u64) -> Self {
        Self::render_only(seed, Patch::ALL)
    }

    /// Renders just `patches` (others stay empty and play silence).
    #[must_use]
    pub fn render_only(seed: u64, patches: &[Patch]) -> Self {
        let jobs: Vec<_> = patches.to_vec();
        let mut rendered = vec![Vec::new(); Patch::ALL.len()];
        #[cfg(target_os = "emscripten")]
        let results: Vec<_> = jobs
            .iter()
            .map(|&patch| (patch, render_variations(seed, patch)))
            .collect();
        #[cfg(not(target_os = "emscripten"))]
        let results: Vec<(Patch, Vec<Box<[f32]>>)> = {
            let workers = thread::available_parallelism()
                .map_or(2, std::num::NonZero::get)
                .clamp(1, 8);
            thread::scope(|scope| {
                #[allow(
                    clippy::needless_collect,
                    reason = "every worker must be spawned before any is joined"
                )]
                let handles: Vec<_> = (0..workers)
                    .map(|worker| {
                        let jobs = &jobs;
                        scope.spawn(move || {
                            jobs.iter()
                                .skip(worker)
                                .step_by(workers)
                                .map(|&patch| (patch, render_variations(seed, patch)))
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .flat_map(|handle| handle.join().unwrap_or_default())
                    .collect()
            })
        };
        for (patch, variations) in results {
            rendered[patch.index()] = variations;
        }
        Self { patches: rendered }
    }

    /// Variation `variation` of `patch` (wrapping), or `None` if it was not rendered.
    #[must_use]
    pub fn get(&self, patch: Patch, variation: usize) -> Option<&[f32]> {
        let variations = self.patches.get(patch.index())?;
        if variations.is_empty() {
            return None;
        }
        variations
            .get(variation % variations.len())
            .map(AsRef::as_ref)
    }

    /// Number of rendered variations of `patch`.
    #[must_use]
    pub fn variations(&self, patch: Patch) -> usize {
        self.patches.get(patch.index()).map_or(0, Vec::len)
    }

    /// Total samples held (memory use is four bytes each).
    #[must_use]
    #[cfg(any(test, not(target_os = "emscripten")))]
    pub fn total_samples(&self) -> usize {
        self.patches
            .iter()
            .flatten()
            .map(|buffer| buffer.len())
            .sum()
    }
}

fn render_variations(seed: u64, patch: Patch) -> Vec<Box<[f32]>> {
    (0..patch.info().variations)
        .map(|variation| render_patch(patch, variation_seed(seed, patch, variation)).into())
        .collect()
}
