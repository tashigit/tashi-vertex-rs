use std::{ffi::c_void, mem::MaybeUninit};

use crate::error::TVResult;
use crate::ptr::Pointer;

/// Container of options for Tashi Vertex.
pub struct Options {
    handle: Pointer<TVOptions>,
}

impl Default for Options {
    fn default() -> Self {
        Options::new()
    }
}

impl Options {
    /// Initializes a default set of options.
    pub fn new() -> Self {
        let mut handle = MaybeUninit::<Pointer<TVOptions>>::uninit();

        let res = unsafe { tv_options_new(handle.as_mut_ptr()) };

        // PANIC: only fails if null is passed to the function above
        res.assert_ok();

        let handle = unsafe { handle.assume_init() };

        Self { handle }
    }

    /// Sets the base minimum event interval (in microseconds).
    pub fn set_base_min_event_interval_us(&mut self, interval: u64) {
        unsafe { tv_options_set_base_min_event_interval_us(self.handle.as_ptr(), interval) }
            .assert_ok();
    }

    /// Enables or disables reporting of gossip events.
    pub fn set_report_gossip_events(&mut self, enabled: bool) {
        unsafe { tv_options_set_report_gossip_events(self.handle.as_ptr(), enabled) }.assert_ok();
    }

    /// Sets the number of seconds a creator can fall behind before being kicked.
    ///
    /// If a creator falls behind for this many seconds or more, we will vote to kick them.
    ///
    /// If a negative value is passed for `seconds`, we will never vote to kick.
    ///
    pub fn set_fallen_behind_kick_s(&mut self, kick: i64) {
        unsafe { tv_options_set_fallen_behind_kick_s(self.handle.as_ptr(), kick) }.assert_ok();
    }

    /// Sets the heartbeat interval (in microseconds).
    ///
    /// When there is no data to finalize, we create empty events at this interval to keep the session
    /// alive.
    ///
    /// Defaults to 500 milliseconds.
    ///
    pub fn set_heartbeat_us(&mut self, heartbeat: u64) {
        unsafe { tv_options_set_heartbeat_us(self.handle.as_ptr(), heartbeat) }.assert_ok();
    }

    /// Sets the target acknowledgment latency (in milliseconds).
    ///
    /// As throughput across the session increases, the ack latency increases.
    /// When the ack latency rises above this threshold, we vote that throughput
    /// across the session should not increase further.
    ///
    /// If this threshold is lower than our uncongested ping, then we'll erroneously always
    /// vote to restrict throughput.
    ///
    /// Defaults to 400 milliseconds.
    ///
    pub fn set_target_ack_latency_ms(&mut self, latency: u32) {
        unsafe { tv_options_set_target_ack_latency_ms(self.handle.as_ptr(), latency) }.assert_ok();
    }

    /// Sets the maximum acknowledgment latency (in milliseconds).
    ///
    /// If ack latency rises above this threshold, we vote to gradually reduce throughput
    /// across the session to bring it down.
    ///
    /// Defaults to 600 milliseconds.
    ///
    pub fn set_max_ack_latency_ms(&mut self, latency: u32) {
        unsafe { tv_options_set_max_ack_latency_ms(self.handle.as_ptr(), latency) }.assert_ok();
    }

    /// Sets the throttle acknowledgment latency (in milliseconds).
    ///
    /// If ack latency rises above this threshold, we vote to drastically restrict throughput
    /// across the session as an emergency measure.
    ///
    /// Defaults to 900 milliseconds.
    ///
    pub fn set_throttle_ack_latency_ms(&mut self, latency: u32) {
        unsafe { tv_options_set_throttle_ack_latency_ms(self.handle.as_ptr(), latency) }
            .assert_ok();
    }

    /// Sets the reset acknowledgment latency (in milliseconds).
    ///
    /// If ack latency rises above this threshold, we vote to reset throughput restriction to its initial
    /// value. This is a last-ditch effort to recover from rising ack latency.
    ///
    /// Defaults to 2000 ms.
    ///
    pub fn set_reset_ack_latency_ms(&mut self, latency: u32) {
        unsafe { tv_options_set_reset_ack_latency_ms(self.handle.as_ptr(), latency) }.assert_ok();
    }

    /// Enables or disables dynamic epoch sizing.
    ///
    /// If `true`, we will vote to resize the epoch depending on network conditions.
    ///
    /// Defaults to `true`.
    ///
    /// Depending on network conditions, rounds may pass more quickly or more slowly.
    ///
    /// Whenever a creator joins or leaves, they'll have to wait out the epoch before the
    /// address book change takes effect.
    ///
    /// A leaving creator doesn't want to wait too long, and a joining creator needs a sufficiently long
    /// window in which to join.
    ///
    /// Creators who don't disable this config option will automatically vote to keep epoch lengths in
    /// the range of 1 to 3 seconds.
    ///
    pub fn set_enable_dynamic_epoch_size(&mut self, enabled: bool) {
        unsafe { tv_options_set_enable_dynamic_epoch_size(self.handle.as_ptr(), enabled) }
            .assert_ok();
    }

    /// Sets the transaction channel size.
    ///
    /// The maximum number of transactions to buffer before applying backpressure.
    ///
    /// Defaults to 32.
    ///
    pub fn set_transaction_channel_size(&mut self, size: usize) {
        unsafe { tv_options_set_transaction_channel_size(self.handle.as_ptr(), size) }.assert_ok();
    }

    /// Sets the maximum number of unacknowledged bytes.
    ///
    /// How many bytes worth of transactions that haven't yet been seen by
    /// the network to pull from the transaction buffer.
    ///
    /// Defaults to `500 MiB`.
    ///
    pub fn set_max_unacknowledged_bytes(&mut self, bytes: usize) {
        unsafe { tv_options_set_max_unacknowledged_bytes(self.handle.as_ptr(), bytes) }.assert_ok();
    }

    /// Sets the maximum number of blocking verify threads.
    ///
    /// Above a constant threshold, signature verifications are sent to a blocking thread pool
    /// instead of using spare compute time in Tokio's core thread pool.
    ///
    /// This sets the maximum number of threads to spawn for blocking verifications.
    ///
    /// It cannot be zero or else events that grow larger than the threshold cannot be verified.
    ///
    /// Defaults to the number of CPU cores available.
    ///
    pub fn set_max_blocking_verify_threads(&mut self, threads: usize) {
        unsafe { tv_options_set_max_blocking_verify_threads(self.handle.as_ptr(), threads) }
            .assert_ok();
    }

    /// Enables or disables state sharing.
    ///
    /// Defaults to `false`.
    ///
    pub fn set_enable_state_sharing(&mut self, enabled: bool) {
        unsafe { tv_options_set_enable_state_sharing(self.handle.as_ptr(), enabled) }.assert_ok();
    }

    /// Sets the number of epoch states to cache.
    ///
    /// When state sharing is enabled, this is the number of epoch states to cache.
    ///
    /// If a fallen behind creator fails to download an epoch's state in time,
    /// they will have to restart the download.
    ///
    /// Defaults to 3.
    ///
    pub fn set_epoch_states_to_cache(&mut self, epochs: u16) {
        unsafe { tv_options_set_epoch_states_to_cache(self.handle.as_ptr(), epochs) }.assert_ok();
    }

    /// Enables or disables hole punching.
    ///
    /// If `true`, we will attempt to use UDP hole punching to establish
    /// direct connections between creators behind NATs.
    ///
    /// Defaults to `true`.
    ///
    pub fn set_enable_hole_punching(&mut self, enabled: bool) {
        unsafe { tv_options_set_enable_hole_punching(self.handle.as_ptr(), enabled) }.assert_ok();
    }

    /// Gets the base minimum event interval (in microseconds).
    pub fn get_base_min_event_interval_us(&self) -> u64 {
        let mut interval = 0;

        unsafe { tv_options_get_base_min_event_interval_us(self.handle.as_ptr(), &mut interval) }
            .assert_ok();

        interval
    }

    /// Gets whether reporting of gossip events is enabled.
    pub fn get_report_gossip_events(&self) -> bool {
        let mut enabled = false;

        unsafe { tv_options_get_report_gossip_events(self.handle.as_ptr(), &mut enabled) }
            .assert_ok();

        enabled
    }

    /// Gets the number of seconds a creator can fall behind before being kicked.
    pub fn get_fallen_behind_kick_s(&self) -> i64 {
        let mut seconds = 0;

        unsafe { tv_options_get_fallen_behind_kick_s(self.handle.as_ptr(), &mut seconds) }
            .assert_ok();

        seconds
    }

    /// Gets the heartbeat interval (in microseconds).
    pub fn get_heartbeat_us(&self) -> u64 {
        let mut interval = 0;

        unsafe { tv_options_get_heartbeat_us(self.handle.as_ptr(), &mut interval) }.assert_ok();

        interval
    }

    /// Gets the target acknowledgment latency (in milliseconds).
    pub fn get_target_ack_latency_ms(&self) -> u32 {
        let mut latency = 0;

        unsafe { tv_options_get_target_ack_latency_ms(self.handle.as_ptr(), &mut latency) }
            .assert_ok();

        latency
    }

    /// Gets the maximum acknowledgment latency (in milliseconds).
    pub fn get_max_ack_latency_ms(&self) -> u32 {
        let mut latency = 0;

        unsafe { tv_options_get_max_ack_latency_ms(self.handle.as_ptr(), &mut latency) }
            .assert_ok();

        latency
    }

    /// Gets the throttle acknowledgment latency (in milliseconds).
    pub fn get_throttle_ack_latency_ms(&self) -> u32 {
        let mut latency = 0;

        unsafe { tv_options_get_throttle_ack_latency_ms(self.handle.as_ptr(), &mut latency) }
            .assert_ok();

        latency
    }

    /// Gets the reset acknowledgment latency (in milliseconds).
    pub fn get_reset_ack_latency_ms(&self) -> u32 {
        let mut latency = 0;

        unsafe { tv_options_get_reset_ack_latency_ms(self.handle.as_ptr(), &mut latency) }
            .assert_ok();

        latency
    }

    /// Gets whether dynamic epoch sizing is enabled.
    pub fn get_enable_dynamic_epoch_size(&self) -> bool {
        let mut enabled = false;

        unsafe { tv_options_get_enable_dynamic_epoch_size(self.handle.as_ptr(), &mut enabled) }
            .assert_ok();

        enabled
    }

    /// Gets the transaction channel size.
    pub fn get_transaction_channel_size(&self) -> usize {
        let mut size = 0;

        unsafe { tv_options_get_transaction_channel_size(self.handle.as_ptr(), &mut size) }
            .assert_ok();

        size
    }

    /// Gets the maximum number of unacknowledged bytes.
    pub fn get_max_unacknowledged_bytes(&self) -> usize {
        let mut bytes = 0;

        unsafe { tv_options_get_max_unacknowledged_bytes(self.handle.as_ptr(), &mut bytes) }
            .assert_ok();

        bytes
    }

    /// Gets the maximum number of blocking verify threads.
    pub fn get_max_blocking_verify_threads(&self) -> usize {
        let mut threads = 0;

        unsafe { tv_options_get_max_blocking_verify_threads(self.handle.as_ptr(), &mut threads) }
            .assert_ok();

        threads
    }

    /// Gets whether state sharing is enabled.
    pub fn get_enable_state_sharing(&self) -> bool {
        let mut enabled = false;

        unsafe { tv_options_get_enable_state_sharing(self.handle.as_ptr(), &mut enabled) }
            .assert_ok();

        enabled
    }

    /// Gets the number of epoch states to cache.
    pub fn get_epoch_states_to_cache(&self) -> u16 {
        let mut epochs = 0;

        unsafe { tv_options_get_epoch_states_to_cache(self.handle.as_ptr(), &mut epochs) }
            .assert_ok();

        epochs
    }

    /// Gets whether hole punching is enabled.
    pub fn get_enable_hole_punching(&self) -> bool {
        let mut enabled = false;

        unsafe { tv_options_get_enable_hole_punching(self.handle.as_ptr(), &mut enabled) }
            .assert_ok();

        enabled
    }
}

pub(crate) type TVOptions = c_void;

unsafe extern "C" {
    fn tv_options_new(options: *mut Pointer<TVOptions>) -> TVResult;

    fn tv_options_set_base_min_event_interval_us(
        options: *mut TVOptions,
        interval: u64,
    ) -> TVResult;

    fn tv_options_set_report_gossip_events(options: *mut TVOptions, enabled: bool) -> TVResult;

    fn tv_options_set_fallen_behind_kick_s(options: *mut TVOptions, kick: i64) -> TVResult;

    fn tv_options_set_heartbeat_us(options: *mut TVOptions, heartbeat: u64) -> TVResult;

    fn tv_options_set_target_ack_latency_ms(options: *mut TVOptions, latency: u32) -> TVResult;

    fn tv_options_set_max_ack_latency_ms(options: *mut TVOptions, latency: u32) -> TVResult;

    fn tv_options_set_throttle_ack_latency_ms(options: *mut TVOptions, latency: u32) -> TVResult;

    fn tv_options_set_reset_ack_latency_ms(options: *mut TVOptions, latency: u32) -> TVResult;

    fn tv_options_set_enable_dynamic_epoch_size(options: *mut TVOptions, enabled: bool)
    -> TVResult;

    fn tv_options_set_transaction_channel_size(options: *mut TVOptions, size: usize) -> TVResult;

    fn tv_options_set_max_unacknowledged_bytes(options: *mut TVOptions, bytes: usize) -> TVResult;

    fn tv_options_set_max_blocking_verify_threads(
        options: *mut TVOptions,
        threads: usize,
    ) -> TVResult;

    fn tv_options_set_enable_state_sharing(options: *mut TVOptions, enabled: bool) -> TVResult;

    fn tv_options_set_epoch_states_to_cache(options: *mut TVOptions, epochs: u16) -> TVResult;

    fn tv_options_set_enable_hole_punching(options: *mut TVOptions, enabled: bool) -> TVResult;

    fn tv_options_get_base_min_event_interval_us(
        options: *const TVOptions,
        interval: *mut u64,
    ) -> TVResult;

    fn tv_options_get_report_gossip_events(
        options: *const TVOptions,
        enabled: *mut bool,
    ) -> TVResult;

    fn tv_options_get_fallen_behind_kick_s(
        options: *const TVOptions,
        seconds: *mut i64,
    ) -> TVResult;

    fn tv_options_get_heartbeat_us(options: *const TVOptions, interval: *mut u64) -> TVResult;

    fn tv_options_get_target_ack_latency_ms(
        options: *const TVOptions,
        latency: *mut u32,
    ) -> TVResult;

    fn tv_options_get_max_ack_latency_ms(options: *const TVOptions, latency: *mut u32) -> TVResult;

    fn tv_options_get_throttle_ack_latency_ms(
        options: *const TVOptions,
        latency: *mut u32,
    ) -> TVResult;

    fn tv_options_get_reset_ack_latency_ms(
        options: *const TVOptions,
        latency: *mut u32,
    ) -> TVResult;

    fn tv_options_get_enable_dynamic_epoch_size(
        options: *const TVOptions,
        enabled: *mut bool,
    ) -> TVResult;

    fn tv_options_get_transaction_channel_size(
        options: *const TVOptions,
        size: *mut usize,
    ) -> TVResult;

    fn tv_options_get_max_unacknowledged_bytes(
        options: *const TVOptions,
        bytes: *mut usize,
    ) -> TVResult;

    fn tv_options_get_max_blocking_verify_threads(
        options: *const TVOptions,
        threads: *mut usize,
    ) -> TVResult;

    fn tv_options_get_enable_state_sharing(
        options: *const TVOptions,
        enabled: *mut bool,
    ) -> TVResult;

    fn tv_options_get_epoch_states_to_cache(
        options: *const TVOptions,
        epochs: *mut u16,
    ) -> TVResult;

    fn tv_options_get_enable_hole_punching(
        options: *const TVOptions,
        enabled: *mut bool,
    ) -> TVResult;
}
