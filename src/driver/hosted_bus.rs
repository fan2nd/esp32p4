//! Transparent SDIO bus trace for locating acquisition failures on hardware.

use sdio::{
    BlockReadCommand, BlockWriteCommand, BusWidth, ByteReadCommand, ByteWriteCommand,
    ControlCommand, MmcBus, MmcError, TuningOp,
};

pub struct TraceBus<B> {
    inner: B,
}

impl<B> TraceBus<B> {
    pub fn new(inner: B) -> Self {
        Self { inner }
    }
}

impl<B: MmcBus> MmcBus for TraceBus<B> {
    async fn init_idle(&mut self, hz: u32) -> Result<(), MmcError> {
        log::info!("sdio bus: init_idle {} Hz", hz);
        let result = self.inner.init_idle(hz).await;
        log::info!("sdio bus: init_idle result {:?}", result);
        result
    }

    fn set_bus(&mut self, width: BusWidth, hz: u32) -> Result<(), MmcError> {
        log::info!("sdio bus: set_bus {:?} {} Hz", width, hz);
        let result = self.inner.set_bus(width, hz);
        log::info!("sdio bus: set_bus result {:?}", result);
        result
    }

    async fn send_command<'a, C>(&mut self, cmd: C) -> Result<C::Resp<'a>, MmcError>
    where
        C: ControlCommand + 'a,
    {
        log::info!("sdio bus: CMD{} arg={:#010x}", C::INDEX, cmd.arg());
        let result = self.inner.send_command(cmd).await;
        match &result {
            Ok(_) => log::info!("sdio bus: CMD{} OK", C::INDEX),
            Err(error) => log::error!("sdio bus: CMD{} failed: {:?}", C::INDEX, error),
        }
        result
    }

    async fn read_blocks<'a, C>(&mut self, cmd: C, auto_stop: bool) -> Result<C::Resp<'a>, MmcError>
    where
        C: BlockReadCommand + 'a,
    {
        self.inner.read_blocks(cmd, auto_stop).await
    }

    async fn write_blocks<'a, C>(
        &mut self,
        cmd: C,
        auto_stop: bool,
    ) -> Result<C::Resp<'a>, MmcError>
    where
        C: BlockWriteCommand + 'a,
    {
        self.inner.write_blocks(cmd, auto_stop).await
    }

    async fn read_bytes<'a, C>(&mut self, cmd: C) -> Result<C::Resp<'a>, MmcError>
    where
        C: ByteReadCommand + 'a,
    {
        self.inner.read_bytes(cmd).await
    }

    async fn write_bytes<'a, C>(&mut self, cmd: C) -> Result<C::Resp<'a>, MmcError>
    where
        C: ByteWriteCommand + 'a,
    {
        self.inner.write_bytes(cmd).await
    }

    async fn tune_bus<O>(&mut self, width: BusWidth, hz: u32, op: O) -> Result<(), MmcError>
    where
        O: TuningOp,
    {
        self.inner.tune_bus(width, hz, op).await
    }

    async fn wait_for_event(&mut self) -> Result<(), MmcError> {
        self.inner.wait_for_event().await
    }

    fn supports_mmc(&self) -> bool {
        self.inner.supports_mmc()
    }

    fn supports_auto_stop(&self) -> bool {
        self.inner.supports_auto_stop()
    }

    fn supports_bus_width(&self) -> BusWidth {
        self.inner.supports_bus_width()
    }

    fn supports_1v8(&self) -> bool {
        self.inner.supports_1v8()
    }

    fn supports_frequency(&self) -> u32 {
        self.inner.supports_frequency()
    }
}
