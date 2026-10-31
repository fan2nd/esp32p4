# Local Rust driver fixes

Based on the published `embassy-usb-synopsys-otg` 0.4.0 crate, used by
esp-hal 1.2.0 originally and retained with esp-hal 1.2.2. Upstream: https://github.com/embassy-rs/embassy .
Licensed under MIT OR Apache-2.0; license texts are included.

1. In `Driver::new`, reserve RX FIFO overhead for every hardware OUT endpoint:
   13 SETUP/control words + one global NAK word + one packet status word +
   two words per endpoint. Keep a larger HAL allowance if supplied. Apply the
   corrected allowance before allocation so allocation limits and MMIO layout agree.
   P4 has 16 endpoint slots: the allowance increases from 31 to 47 words.
2. In `ControlPipe::data_in`, arm the zero-length OUT status packet before
   submitting the final IN packet. P4 clears OUT EPENA after a packet; merely
   clearing NAK does not re-enable reception for the next control transfer.
3. Re-enable OUT after consuming each packet. Handle incomplete ISO transfers:
   retire and flush expired IN data, and advance missed OUT frame parity.
   Enable the corresponding interrupts; remove the old EOPF busy-wait retry.
4. Finish global OUT NAK before disabling an active OUT endpoint. Drain RX
   status through the existing interrupt handler while CPU interrupts are masked,
   so global NAK can take effect. This prevents pending EPDIS on interface reopen.
5. Add opt-in `Config::force_full_speed` (default false) for a high-speed PHY.
   The UAC1 example selects it; the enumeration probe retains high-speed mode.
6. Clear bulk/interrupt halt by cancelling the interrupted transfer and resetting
   DATA0. Disabling OUT discards its buffered packet; disabling IN waits for
   EPDISD or EPENA clearing with a bounded handshake before flushing its FIFO.
   Endpoint disable/enable alone preserves PID and halt, allowing the desktop's
   MSC BOT reset to preserve both until the host sends CLEAR_FEATURE.

No C code is built or linked. The receive FIFO accounting was cross-checked
against the explanation in the upstream TinyUSB DWC2 driver (reference only):
https://github.com/hathach/tinyusb/blob/master/src/portable/synopsys/dwc2/dcd_dwc2.c

The local allocation unit-test module has been removed under the project policy;
production FIFO allocation and USB fixes remain unchanged. Current board
configuration uses full-speed UAC1 on the HS PHY. High-speed audio, microphone
capture and other controller families are not validated by this patch.
Dependency refresh does not add hardware validation. Use firmware builds and
explicit board verification rather than adding local unit tests.
