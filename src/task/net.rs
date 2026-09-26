use core::net::Ipv4Addr;
use core::ptr::{read_volatile, write_volatile};

use embassy_net::{Config, Ipv4Address, Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_time::{Duration, Timer};
use esp_p4_eth::{Device, Ethernet, Runner as EthRunner, StaticDmaResources};
use static_cell::ConstStaticCell;

use crate::board;

const DMA_STATUS_REG: *mut u32 = 0x5009_9014 as *mut u32;
const DMA_RX_POLL_DEMAND_REG: *mut u32 = 0x5009_9008 as *mut u32;
const MAC_CONFIG_REG: *mut u32 = 0x5009_8000 as *mut u32;
const MAC_FRAME_FILTER_REG: *mut u32 = 0x5009_8004 as *mut u32;
const DMA_OP_MODE_REG: *mut u32 = 0x5009_9018 as *mut u32;

#[unsafe(link_section = ".dma_bss")]
static DMA_RESOURCES: ConstStaticCell<StaticDmaResources> =
    ConstStaticCell::new(StaticDmaResources::new());
static NET_RESOURCES: ConstStaticCell<StackResources<4>> =
    ConstStaticCell::new(StackResources::<4>::new());

pub struct Network {
    pub stack: Stack<'static>,
    pub net_runner: Runner<'static, Device<'static>>,
    pub eth_runner: EthRunner<'static>,
    pub ethernet: Ethernet<'static>,
    pub ip: Ipv4Addr,
}

pub fn init() -> Network {
    let dma_resources = DMA_RESOURCES.take();
    let (device, eth_runner, ethernet) =
        esp_p4_eth::new_from_static_resources(board::MAC_ADDR, dma_resources);
    let resources = NET_RESOURCES.take();

    let config = Config::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(to_embassy_ipv4(board::LOCAL_IPV4), board::SUBNET_PREFIX),
        gateway: board::GATEWAY_IPV4.map(to_embassy_ipv4),
        dns_servers: Default::default(),
    });

    let (stack, net_runner) = embassy_net::new(device, config, resources, board::NET_SEED);

    Network {
        stack,
        net_runner,
        eth_runner,
        ethernet,
        ip: board::LOCAL_IPV4,
    }
}

#[embassy_executor::task]
pub async fn drive_network(mut runner: Runner<'static, Device<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
pub async fn maintain_ethernet(stack: Stack<'static>) -> ! {
    let mut workarounds_applied = false;

    loop {
        if !workarounds_applied && stack.is_link_up() {
            apply_receive_workarounds();
            workarounds_applied = true;
        }

        recover_rx_dma();
        Timer::after(Duration::from_millis(1)).await;
    }
}

fn apply_receive_workarounds() {
    // Keep multicast visible to smoltcp and accept small ARP/mDNS frames on P4.
    unsafe {
        write_volatile(MAC_FRAME_FILTER_REG, 0x8000_0011); // RA | PR | PM
        write_volatile(MAC_CONFIG_REG, read_volatile(MAC_CONFIG_REG) & !(1 << 25)); // ACS off
        write_volatile(DMA_OP_MODE_REG, read_volatile(DMA_OP_MODE_REG) | (1 << 6)); // FUF on
    }
}

fn recover_rx_dma() {
    let dma_status = unsafe { read_volatile(DMA_STATUS_REG) };
    let rx_process_state = (dma_status >> 17) & 0b111;

    if rx_process_state == 0 || rx_process_state == 4 || dma_status & (1 << 7) != 0 {
        // DMA_STATUS is write-one-to-clear; RX_POLL_DEMAND restarts the RX scan.
        unsafe {
            write_volatile(DMA_STATUS_REG, dma_status & 0x0001_E7FF);
            write_volatile(DMA_RX_POLL_DEMAND_REG, 1);
        }
    }
}

fn to_embassy_ipv4(ip: Ipv4Addr) -> Ipv4Address {
    let [a, b, c, d] = ip.octets();
    Ipv4Address::new(a, b, c, d)
}
