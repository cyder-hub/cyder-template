#![allow(dead_code)]

use std::{
    sync::Mutex,
    time::{SystemTime, SystemTimeError, UNIX_EPOCH},
};

const DEFAULT_EPOCH_MS: u64 = 1_716_257_820_000;
const SIGNED_ID_BITS: u8 = 63;
const DEFAULT_WORKER_BITS: u8 = 8;
const DEFAULT_SEQUENCE_BITS: u8 = 12;
const DEFAULT_TIMESTAMP_BITS: u8 = SIGNED_ID_BITS - DEFAULT_WORKER_BITS - DEFAULT_SEQUENCE_BITS;

#[derive(Debug, Clone, Copy)]
pub struct IdGeneratorConfig {
    pub epoch_ms: u64,
    pub worker_bits: u8,
    pub sequence_bits: u8,
    pub worker_id: u64,
}

impl IdGeneratorConfig {
    pub fn for_worker(worker_id: u64) -> Self {
        Self {
            worker_id,
            ..Self::default()
        }
    }
}

impl Default for IdGeneratorConfig {
    fn default() -> Self {
        Self {
            epoch_ms: DEFAULT_EPOCH_MS,
            worker_bits: DEFAULT_WORKER_BITS,
            sequence_bits: DEFAULT_SEQUENCE_BITS,
            worker_id: 1,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdError {
    #[error("worker_id {worker_id} exceeds maximum {max_worker_id} for {worker_bits} worker bits")]
    WorkerIdOutOfRange {
        worker_id: u64,
        max_worker_id: u64,
        worker_bits: u8,
    },
    #[error(
        "id bit allocation is invalid: worker_bits={worker_bits}, sequence_bits={sequence_bits}"
    )]
    InvalidBitAllocation { worker_bits: u8, sequence_bits: u8 },
    #[error("system clock is before configured epoch: now_ms={now_ms}, epoch_ms={epoch_ms}")]
    TimestampBeforeEpoch { now_ms: u64, epoch_ms: u64 },
    #[error(
        "system clock moved backwards while generating id: last_elapsed_ms={last_elapsed_ms}, current_elapsed_ms={current_elapsed_ms}"
    )]
    ClockMovedBackwards {
        last_elapsed_ms: u64,
        current_elapsed_ms: u64,
    },
    #[error("system clock error while generating id: {source}")]
    Clock { source: SystemTimeError },
    #[error("generated id exceeds signed 64-bit range")]
    IdOverflow,
}

#[derive(Debug)]
struct IdState {
    last_elapsed_ms: u64,
    sequence: u64,
}

#[derive(Debug)]
pub struct IdGenerator {
    config: IdGeneratorConfig,
    max_worker_id: u64,
    max_sequence: u64,
    worker_shift: u8,
    timestamp_shift: u8,
    state: Mutex<IdState>,
}

impl IdGenerator {
    pub fn new(config: IdGeneratorConfig) -> Result<Self, IdError> {
        validate_bit_allocation(config.worker_bits, config.sequence_bits)?;

        let max_worker_id = (1_u64 << config.worker_bits) - 1;
        if config.worker_id > max_worker_id {
            return Err(IdError::WorkerIdOutOfRange {
                worker_id: config.worker_id,
                max_worker_id,
                worker_bits: config.worker_bits,
            });
        }

        let max_sequence = (1_u64 << config.sequence_bits) - 1;
        let worker_shift = config.sequence_bits;
        let timestamp_shift = config.worker_bits + config.sequence_bits;

        Ok(Self {
            config,
            max_worker_id,
            max_sequence,
            worker_shift,
            timestamp_shift,
            state: Mutex::new(IdState {
                last_elapsed_ms: 0,
                sequence: 0,
            }),
        })
    }

    pub fn for_worker(worker_id: u64) -> Result<Self, IdError> {
        Self::new(IdGeneratorConfig::for_worker(worker_id))
    }

    pub fn next_id(&self) -> Result<i64, IdError> {
        let elapsed_ms = self.current_elapsed_ms()?;
        self.next_id_for_elapsed_ms(elapsed_ms)
    }

    fn current_elapsed_ms(&self) -> Result<u64, IdError> {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|source| IdError::Clock { source })?
            .as_millis() as u64;

        now_ms
            .checked_sub(self.config.epoch_ms)
            .ok_or(IdError::TimestampBeforeEpoch {
                now_ms,
                epoch_ms: self.config.epoch_ms,
            })
    }

    fn next_id_for_elapsed_ms(&self, mut elapsed_ms: u64) -> Result<i64, IdError> {
        let mut state = self.state.lock().expect("id state mutex should not poison");

        if elapsed_ms < state.last_elapsed_ms {
            return Err(IdError::ClockMovedBackwards {
                last_elapsed_ms: state.last_elapsed_ms,
                current_elapsed_ms: elapsed_ms,
            });
        }

        if elapsed_ms == state.last_elapsed_ms {
            if state.sequence >= self.max_sequence {
                elapsed_ms = state.last_elapsed_ms + 1;
                state.sequence = 0;
            } else {
                state.sequence += 1;
            }
        } else {
            state.sequence = 0;
        }

        state.last_elapsed_ms = elapsed_ms;
        self.compose_id(elapsed_ms, state.sequence)
    }

    fn compose_id(&self, elapsed_ms: u64, sequence: u64) -> Result<i64, IdError> {
        let id = (elapsed_ms << self.timestamp_shift)
            | (self.config.worker_id << self.worker_shift)
            | sequence;

        i64::try_from(id).map_err(|_| IdError::IdOverflow)
    }

    #[cfg(test)]
    fn max_worker_id(&self) -> u64 {
        self.max_worker_id
    }
}

fn validate_bit_allocation(worker_bits: u8, sequence_bits: u8) -> Result<(), IdError> {
    if worker_bits == 0 || sequence_bits == 0 || worker_bits + sequence_bits >= 63 {
        return Err(IdError::InvalidBitAllocation {
            worker_bits,
            sequence_bits,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_is_43_8_12() {
        assert_eq!(DEFAULT_TIMESTAMP_BITS, 43);
        assert_eq!(DEFAULT_WORKER_BITS, 8);
        assert_eq!(DEFAULT_SEQUENCE_BITS, 12);
    }

    #[test]
    fn generated_ids_are_monotonic() {
        let generator = IdGenerator::for_worker(3).expect("worker id should be valid");
        let mut previous = generator.next_id().expect("first id should generate");

        for _ in 0..256 {
            let next = generator.next_id().expect("next id should generate");
            assert!(
                next > previous,
                "ids must increase monotonically: previous={previous}, next={next}"
            );
            previous = next;
        }
    }

    #[test]
    fn worker_id_is_validated_against_worker_bits() {
        let config = IdGeneratorConfig {
            worker_id: 256,
            worker_bits: 8,
            ..IdGeneratorConfig::default()
        };

        let error = IdGenerator::new(config).expect_err("worker id should be rejected");
        assert!(matches!(
            error,
            IdError::WorkerIdOutOfRange {
                worker_id: 256,
                max_worker_id: 255,
                worker_bits: 8
            }
        ));

        let generator = IdGenerator::for_worker(255).expect("max worker id should be accepted");
        assert_eq!(generator.max_worker_id(), 255);
    }

    #[test]
    fn clock_rollback_returns_error() {
        let generator = IdGenerator::for_worker(1).expect("worker id should be valid");

        let first = generator
            .next_id_for_elapsed_ms(10)
            .expect("first id should generate");
        assert!(first > 0);

        let error = generator
            .next_id_for_elapsed_ms(9)
            .expect_err("rollback should fail");

        assert!(matches!(
            error,
            IdError::ClockMovedBackwards {
                last_elapsed_ms: 10,
                current_elapsed_ms: 9
            }
        ));
    }
}
