use std::sync::{mpsc, Arc, Mutex};
use std::sync::mpsc::{Sender, TryRecvError};
use std::time::SystemTime;
use bitflags::{bitflags, bitflags_match};
use bytemuck::{Pod, Zeroable};
use color_eyre::eyre;
use evdev::{AbsoluteAxisCode, AbsoluteAxisEvent, AttributeSet, BusType, FFEffect, FFEffectCode, InputId, KeyCode, KeyEvent, MiscCode, MiscEvent, SynchronizationCode, SynchronizationEvent, UinputAbsSetup};
use evdev::uinput::VirtualDevice;

use hidapi::{DeviceInfo, HidDevice};

fn main() -> eyre::Result<()> {
    let api = hidapi::HidApi::new()?;

    let mut devices: Vec<DeviceInfo> = Vec::new();
    for device in api.device_list() {
        if !devices.iter().any(|existing_device| existing_device.path() == device.path()) {
            devices.push(device.clone());
        }
    }

    let steam_controllers = devices.iter()
        .filter(|device| device.vendor_id() == 0x28de && device.product_id() == 0x1304)
        .cloned()
        .collect::<Vec<DeviceInfo>>();

    let mut controllers: Vec<HidDevice> = Vec::new();
    for device in steam_controllers.iter() {
        let steam_controller = api.open_path(device.path())?;
        controllers.push(steam_controller);
    }

    let (tx, rx) = mpsc::channel::<DaemonState>();
    let mut daemons_running = controllers.len();

    for controller in controllers {
        let tx = tx.clone();
        std::thread::spawn(move || handle_controller(controller, tx));
    }

    while daemons_running > 0 {
        match rx.recv() {
            Ok(DaemonState::Stopped) => {
                daemons_running -= 1;
            },
            Err(err) => {
                println!("{:?}", err);
                break;
            }
        }
    }

    Ok(())
}

enum DaemonState {
    Stopped,
}

bitflags! {
    // This is actually supposed to be 64 bits long, but all buttons are already accounted for with 32 bits.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Zeroable, Pod)]
    struct Buttons: u32 {
        const BTN_A = 0b00000000000000000000000000000001;
        const BTN_B = 0b00000000000000000000000000000010;
        const BTN_X = 0b00000000000000000000000000000100;
        const BTN_Y = 0b000000000000000000001000;
        const BTN_QUICK_ACCESS = 0b00000000000000000000000000010000;
        const BTN_THUMBR = 0b00000000000000000000000000100000;
        const BTN_SELECT = 0b00000000000000000000000001000000;
        const BTN_R4 = 0b00000000000000000000000010000000;
        const BTN_R5 = 0b00000000000000000000000100000000;
        const BTN_R1 = 0b00000000000000000000001000000000;
        const BTN_DPAD_DOWN = 0b00000000000000000000010000000000;
        const BTN_DPAD_RIGHT = 0b00000000000000000000100000000000;
        const BTN_DPAD_LEFT = 0b00000000000000000001000000000000;
        const BTN_DPAD_UP = 0b00000000000000000010000000000000;
        const BTN_START = 0b00000000000000000100000000000000;
        const BTN_THUMBL = 0b00000000000000001000000000000000;
        const BTN_STEAM = 0b00000000000000010000000000000000;
        const BTN_L4 = 0b00000000000000100000000000000000;
        const BTN_L5 = 0b00000000000001000000000000000000;
        const BTN_L1 = 0b00000000000010000000000000000000;
        const BTN_THUMBR_TOUCH = 0b00000000000100000000000000000000;
        const BTN_RIGHT_PAD_TOUCH = 0b00000000001000000000000000000000;
        const BTN_RIGHT_PAD_CLICK = 0b00000000010000000000000000000000;
        const BTN_R2 = 0b00000000100000000000000000000000;
        const BTN_THUMBL_TOUCH = 0b00000001000000000000000000000000;
        const BTN_LEFT_PAD_TOUCH = 0b00000010000000000000000000000000;
        const BTN_LEFT_PAD_CLICK = 0b00000100000000000000000000000000;
        const BTN_L2 = 0b00001000000000000000000000000000;
        const BTN_GRIPR = 0b00010000000000000000000000000000;
        const BTN_GRIPL = 0b00100000000000000000000000000000;
        const BTN_UNKNOWN_1 = 0b01000000000000000000000000000000;
        const BTN_UNKNOWN_2 = 0b10000000000000000000000000000000;
    }
}

// fn event_time_now() -> eyre::Result<TimeVal> {
//     let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
//     Ok(TimeVal::new(now.as_secs() as i64, now.subsec_micros() as i64))
// }

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Command {
    SetDigitalMappings = 0x80,
    ClearDigitalMappings = 0x81,
    GetDigitalMappings = 0x82,
    SetDefaultMappings = 0x85,
    SetSettings = 0x87,
    GetSettings = 0x88,
    Unknown1 = 0xe2,
    Unknown2 = 0xdc,
    Unknown3 = 0xc1,
}


#[derive(Debug, Clone)]
struct SteamControllerHid {
    device: Arc<Mutex<HidDevice>>,
    device_info: DeviceInfo,
}

impl SteamControllerHid {
    const FEATURE_REPORT_CMD: u8 = 0x01;
    const SETTING_LEFT_TRACKPAD_MODE: u8 = 0x07;
    const SETTING_RIGHT_TRACKPAD_MODE: u8 = 0x08;
    const SETTING_LEFT_TRACKPAD_CLICK_PRESSURE: u8 = 52;
    const SETTING_RIGHT_TRACKPAD_CLICK_PRESSURE: u8 = 53;
    const SETTING_STEAM_WATCHDOG_ENABLE: u8 = 71;

    pub fn new(device: HidDevice) -> Self {
        let device_info = device.get_device_info().unwrap();

        SteamControllerHid {
            device: Arc::new(Mutex::new(device)),
            device_info,
        }
    }

    pub fn get_device_info(&self) -> &DeviceInfo {
        &self.device_info
    }

    pub fn read(&self, buf: &mut [u8]) -> eyre::Result<usize> {
        let device = self.device.lock()
            .map_err(|e| eyre::eyre!("Failed to lock device: {e}"))?;
        let read = device.read(buf)?;
        Ok(read)
    }

    pub fn send_command(&self, command: Command) -> eyre::Result<()> {
        self.send_command_with_payload(command, &[])
    }

    pub fn send_command_with_payload(&self, command: Command, payload: &[u8]) -> eyre::Result<()> {
        let size = payload.len();

        let mut buf = vec![Self::FEATURE_REPORT_CMD, command as u8, size as u8];
        buf.extend_from_slice(payload);

        while buf.len() < 64 {
            buf.push(0x00);
        }

        let device = self.device.lock()
            .map_err(|e| eyre::eyre!("Failed to lock device: {e}"))?;

        let mut retries = 50;
        while let Err(e) = device.send_feature_report(&buf) {
            retries -= 1;
            if retries <=0 {
                return Err(eyre::eyre!("Failed to send feature report after 50 retries: {}", e));
            }
        }

        Ok(())
    }
}

// copied from https://github.com/torvalds/linux/blob/50897c955902c93ae71c38698abb910525ebdc89/drivers/hid/hid-steam.c#L1357
fn i16_from_le_bytes_steam(bytes: &[u8]) -> eyre::Result<i16> {
    let mut short = i16::from_le_bytes(bytes.try_into()?);

    if short == i16::MIN {
        short = i16::MIN + 1;
    }

    Ok(short)
}

fn handle_controller(controller: HidDevice, tx: Sender<DaemonState>) {
    let mut virtual_controller = None;
    let controller = SteamControllerHid::new(controller);

    let (write_thread_tx, rx) = mpsc::channel::<bool>();

    let write_thread_params = (
        controller.clone(),
    );
    let write_thread = std::thread::spawn(move || {
        let (controller, ) = write_thread_params;

        controller.send_command(Command::ClearDigitalMappings).unwrap();

        // Enable Motion Sensors.
        controller.send_command_with_payload(Command::SetSettings, &[
            0x18, 0x00, 0x00,
            0x2e, 0x00, 0x00,
            0x34, 0xff, 0xff,
            0x35, 0xff, 0xff,
            0x2e, 0x00, 0x00,
        ]).unwrap();

        loop {
            // Disable lizard mode.
            controller.send_command_with_payload(Command::SetSettings, &[
                0x09, 0x00, 0x00,
            ]).unwrap();

            match rx.try_recv() {
                Ok(should_exit) => {
                    if should_exit {
                        return;
                    }
                }
                Err(err) => {
                    match err {
                        TryRecvError::Disconnected => {
                            return;
                        }
                        _ => {}
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });

    let mut buf = [0u8; 1024];

    let mut last_button_state = Buttons::empty();

    loop {
        let read = controller.read(&mut buf);
        if let Ok(read) = read {
            // println!("{:?}", &buf[..read]);

            if virtual_controller.is_none() {
                virtual_controller = Some(VirtualController::new(&controller.get_device_info()).unwrap());
            }
            let virtual_controller = virtual_controller.as_mut().unwrap();

            let report_id = buf[0];

            if report_id == 66 {
                let buttons_u32 = u32::from_le_bytes(buf[2..6].try_into().unwrap());

                let buttons = Buttons::from_bits_truncate(buttons_u32);

                let released = last_button_state.difference(buttons);
                let pressed = buttons.difference(last_button_state);
                last_button_state = buttons;

                if !pressed.is_empty() || !released.is_empty() {
                    released.iter().for_each(|button| {
                        virtual_controller.send_button_events(button, false).unwrap();
                    });
                    pressed.iter().for_each(|button| {
                        virtual_controller.send_button_events(button, true).unwrap();
                    });
                }

                let left_joystick_x = i16_from_le_bytes_steam(&buf[10..12]).unwrap();
                let left_joystick_y = i16_from_le_bytes_steam(&buf[12..14]).unwrap();
                let right_joystick_x = i16_from_le_bytes_steam(&buf[14..16]).unwrap();
                let right_joystick_y = i16_from_le_bytes_steam(&buf[16..18]).unwrap();

                virtual_controller.send_joystick_events(left_joystick_x, left_joystick_y, right_joystick_x, right_joystick_y).unwrap();

                let left_pad_x = i16_from_le_bytes_steam(&buf[18..20]).unwrap();
                let left_pad_y = i16_from_le_bytes_steam(&buf[20..22]).unwrap();
                let left_pad_pressure = i16_from_le_bytes_steam(&buf[22..24]).unwrap();

                let right_pad_x = i16_from_le_bytes_steam(&buf[24..26]).unwrap();
                let right_pad_y = i16_from_le_bytes_steam(&buf[26..28]).unwrap();
                let right_pad_pressure = i16_from_le_bytes_steam(&buf[28..30]).unwrap();

                virtual_controller.send_pad_events(
                    left_pad_x, left_pad_y, left_pad_pressure,
                    right_pad_x, right_pad_y, right_pad_pressure,
                ).unwrap();

                // Not sure if this is actually the accelerometer.
                let accel_x = i16_from_le_bytes_steam(&buf[30..32]).unwrap();
                let accel_y = i16_from_le_bytes_steam(&buf[32..34]).unwrap();
                let accel_z = i16_from_le_bytes_steam(&buf[34..36]).unwrap();
                // println!("Accel: {:?} {:?} {:?}", accel_x, accel_y, accel_z);

                let gyro_x = i16_from_le_bytes_steam(&buf[36..38]).unwrap();
                let gyro_y = i16_from_le_bytes_steam(&buf[38..40]).unwrap();
                let gyro_z = i16_from_le_bytes_steam(&buf[40..42]).unwrap();

                // TODO: Figure out the other sensors
                virtual_controller.send_motion_sensor_events(
                    gyro_x, gyro_y, gyro_z,
                ).unwrap();

                // println!("{:?}", &buf[42..read]);
            } else {
                // println!("{:?}", &buf[..read]);
            }

            virtual_controller.sync().unwrap();
        } else {
            println!("{:?}", read);

            tx.send(DaemonState::Stopped).unwrap();
            break;
        }
    }

    write_thread_tx.send(true).unwrap();
    write_thread.join().unwrap();
}

#[derive(Debug)]
struct VirtualController {
    gamepad: VirtualDevice,
    motion_sensors: VirtualDevice,
    motion_sensor_timestamp_us: i32,
}

impl VirtualController {
    pub fn new(device_info: &DeviceInfo) -> eyre::Result<Self> {
        let mut buttons = AttributeSet::<KeyCode>::new();
        buttons.insert(KeyCode::BTN_TR2);
        buttons.insert(KeyCode::BTN_TL2);
        buttons.insert(KeyCode::BTN_TR);
        buttons.insert(KeyCode::BTN_TL);
        buttons.insert(KeyCode::BTN_SOUTH);
        buttons.insert(KeyCode::BTN_EAST);
        buttons.insert(KeyCode::BTN_WEST);
        buttons.insert(KeyCode::BTN_NORTH);
        buttons.insert(KeyCode::BTN_0);
        buttons.insert(KeyCode::BTN_1);
        buttons.insert(KeyCode::BTN_2);
        buttons.insert(KeyCode::BTN_3);
        buttons.insert(KeyCode::BTN_4);
        buttons.insert(KeyCode::BTN_5);
        buttons.insert(KeyCode::BTN_6);
        buttons.insert(KeyCode::BTN_7);
        buttons.insert(KeyCode::BTN_DPAD_UP);
        buttons.insert(KeyCode::BTN_DPAD_RIGHT);
        buttons.insert(KeyCode::BTN_DPAD_LEFT);
        buttons.insert(KeyCode::BTN_DPAD_DOWN);
        buttons.insert(KeyCode::BTN_SELECT);
        buttons.insert(KeyCode::BTN_BASE);
        buttons.insert(KeyCode::BTN_MODE);
        buttons.insert(KeyCode::BTN_START);
        buttons.insert(KeyCode::BTN_THUMBR);
        buttons.insert(KeyCode::BTN_THUMBL);

        let abs_info = evdev::AbsInfo::new(0, -32767, 32767, 0, 0, 6553);
        let abs_x = UinputAbsSetup::new(AbsoluteAxisCode::ABS_X, abs_info);
        let abs_y = UinputAbsSetup::new(AbsoluteAxisCode::ABS_Y, abs_info);
        let abs_rx = UinputAbsSetup::new(AbsoluteAxisCode::ABS_RX, abs_info);
        let abs_ry = UinputAbsSetup::new(AbsoluteAxisCode::ABS_RY, abs_info);

        let abs_info = evdev::AbsInfo::new(0, -32767, 32767, 256, 0, 0);
        let abs_hat0x = UinputAbsSetup::new(AbsoluteAxisCode::ABS_HAT0X, abs_info);
        let abs_hat0y = UinputAbsSetup::new(AbsoluteAxisCode::ABS_HAT0Y, abs_info);
        let abs_hat1x = UinputAbsSetup::new(AbsoluteAxisCode::ABS_HAT1X, abs_info);
        let abs_hat1y = UinputAbsSetup::new(AbsoluteAxisCode::ABS_HAT1Y, abs_info);

        let abs_info = evdev::AbsInfo::new(0, 0, 32767, 0, 0, 5461);
        let abs_hat2x = UinputAbsSetup::new(AbsoluteAxisCode::ABS_HAT2X, abs_info);
        let abs_hat2y = UinputAbsSetup::new(AbsoluteAxisCode::ABS_HAT2Y, abs_info);

        let input_id = InputId::new(
            BusType(device_info.bus_type() as u16),
            device_info.vendor_id(),
            device_info.product_id(),
            device_info.release_number(),
        );

        let gamepad = VirtualDevice::builder()?
            .name(&format!("Steam Controller (evdev wrapper for {:?})", device_info.path()))
            .input_id(input_id.clone())
            .with_phys(device_info.path())?
            .with_keys(&buttons)?
            .with_absolute_axis(&abs_x)?
            .with_absolute_axis(&abs_y)?
            .with_absolute_axis(&abs_rx)?
            .with_absolute_axis(&abs_ry)?
            .with_absolute_axis(&abs_hat0x)?
            .with_absolute_axis(&abs_hat0y)?
            .with_absolute_axis(&abs_hat1x)?
            .with_absolute_axis(&abs_hat1y)?
            .with_absolute_axis(&abs_hat2x)?
            .with_absolute_axis(&abs_hat2y)?
            .with_ff(&AttributeSet::from_iter([FFEffectCode::FF_RUMBLE]))?
            .with_ff_effects_max(32)
            .build()?;

        let abs_info = evdev::AbsInfo::new(0, -32768, 32768, 32, 0, 16384);
        let accel_x = UinputAbsSetup::new(AbsoluteAxisCode::ABS_X, abs_info);
        let accel_y = UinputAbsSetup::new(AbsoluteAxisCode::ABS_Y, abs_info);
        let accel_z = UinputAbsSetup::new(AbsoluteAxisCode::ABS_Z, abs_info);

        let abs_info = evdev::AbsInfo::new(0, -32768, 32768, 1, 0, 16);
        let gyro_x = UinputAbsSetup::new(AbsoluteAxisCode::ABS_RX, abs_info);
        let gyro_y = UinputAbsSetup::new(AbsoluteAxisCode::ABS_RY, abs_info);
        let gyro_z = UinputAbsSetup::new(AbsoluteAxisCode::ABS_RZ, abs_info);

        let motion_sensors = VirtualDevice::builder()?
            .name(&format!("Steam Controller (Motion Sensors) (evdev wrapper for {:?})", device_info.path()))
            .input_id(input_id)
            .with_phys(device_info.path())?
            .with_msc(&AttributeSet::from_iter([MiscCode::MSC_TIMESTAMP]))?
            .with_absolute_axis(&accel_x)?
            .with_absolute_axis(&accel_y)?
            .with_absolute_axis(&accel_z)?
            .with_absolute_axis(&gyro_x)?
            .with_absolute_axis(&gyro_y)?
            .with_absolute_axis(&gyro_z)?
            .build()?;

        Ok(
            VirtualController {
                gamepad,
                motion_sensors,
                motion_sensor_timestamp_us: 0,
            }
        )
    }

    pub fn send_button_events(&mut self, buttons: Buttons, pressed: bool) -> eyre::Result<()> {
        let key_code = bitflags_match!(buttons, {
            Buttons::BTN_A => Some(KeyCode::BTN_SOUTH),
            Buttons::BTN_B => Some(KeyCode::BTN_EAST),
            Buttons::BTN_X => Some(KeyCode::BTN_WEST),
            Buttons::BTN_Y => Some(KeyCode::BTN_NORTH),
            Buttons::BTN_R1 => Some(KeyCode::BTN_TR),
            Buttons::BTN_L1 => Some(KeyCode::BTN_TL),
            Buttons::BTN_R2 => Some(KeyCode::BTN_TR2),
            Buttons::BTN_L2 => Some(KeyCode::BTN_TL2),
            Buttons::BTN_R4 => Some(KeyCode::BTN_0),
            Buttons::BTN_L4 => Some(KeyCode::BTN_1),
            Buttons::BTN_R5 => Some(KeyCode::BTN_2),
            Buttons::BTN_L5 => Some(KeyCode::BTN_3),
            Buttons::BTN_RIGHT_PAD_CLICK => Some(KeyCode::BTN_4),
            Buttons::BTN_LEFT_PAD_CLICK => Some(KeyCode::BTN_5),
            Buttons::BTN_GRIPR => Some(KeyCode::BTN_6),
            Buttons::BTN_GRIPL => Some(KeyCode::BTN_7),
            Buttons::BTN_THUMBL => Some(KeyCode::BTN_THUMBL),
            Buttons::BTN_THUMBR => Some(KeyCode::BTN_THUMBR),
            Buttons::BTN_DPAD_UP => Some(KeyCode::BTN_DPAD_UP),
            Buttons::BTN_DPAD_DOWN => Some(KeyCode::BTN_DPAD_DOWN),
            Buttons::BTN_DPAD_LEFT => Some(KeyCode::BTN_DPAD_LEFT),
            Buttons::BTN_DPAD_RIGHT => Some(KeyCode::BTN_DPAD_RIGHT),
            Buttons::BTN_START => Some(KeyCode::BTN_START),
            Buttons::BTN_SELECT => Some(KeyCode::BTN_SELECT),
            Buttons::BTN_STEAM => Some(KeyCode::BTN_MODE),
            Buttons::BTN_QUICK_ACCESS => Some(KeyCode::BTN_BASE),
            _ => None,
        });

        if let Some(key_code) = key_code {
            let value = if pressed { 1 } else { 0 };
            self.gamepad.emit(&[*KeyEvent::new(key_code, value)])?;
        }

        Ok(())
    }

    pub fn send_joystick_events(&mut self, left_joystick_x: i16, left_joystick_y: i16, right_joystick_x: i16, right_joystick_y: i16) -> eyre::Result<()> {
        let axis_events = vec![
            *AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_X, left_joystick_x as i32),
            *AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_Y, -left_joystick_y as i32),
            *AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_RX, right_joystick_x as i32),
            *AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_RY, -right_joystick_y as i32),
        ];

        self.gamepad.emit(&axis_events)?;

        Ok(())
    }

    pub fn send_pad_events(
        &mut self,
        left_pad_x: i16,
        left_pad_y: i16,
        left_pad_pressure: i16,
        right_pad_x: i16,
        right_pad_y: i16,
        right_pad_pressure: i16,
    ) -> eyre::Result<()> {
        let pad_events = vec![
            *AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_HAT0X, left_pad_x as i32),
            *AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_HAT0Y, left_pad_y as i32),
            *AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_HAT1X, right_pad_x as i32),
            *AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_HAT1Y, right_pad_y as i32),
        ];

        self.gamepad.emit(&pad_events)?;

        Ok(())
    }

    pub fn increment_motion_sensor_timestamp(&mut self, delta_us: i32) {
        if let Some(motion_sensor_timestamp_us) = self.motion_sensor_timestamp_us.checked_add(delta_us) {
            self.motion_sensor_timestamp_us = motion_sensor_timestamp_us;
        } else {
            // Hopefully doing this won't break anything; I have no idea what else to do.
            self.motion_sensor_timestamp_us = 0;
        }
    }

    pub fn send_motion_sensor_events(
        &mut self,
        gyro_x: i16,
        gyro_y: i16,
        gyro_z: i16,
    ) -> eyre::Result<()> {
        // Copied from https://github.com/torvalds/linux/blob/5d6919055dec134de3c40167a490f33c74c12581/drivers/hid/hid-steam.c#L1686
        self.increment_motion_sensor_timestamp(4000);

        let motion_events = vec![
            *MiscEvent::new(MiscCode::MSC_TIMESTAMP, self.motion_sensor_timestamp_us),
            *AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_RX, gyro_x as i32),
            *AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_RY, gyro_y as i32),
            *AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_RZ, gyro_z as i32),
        ];

        self.motion_sensors.emit(&motion_events)?;

        Ok(())
    }

    pub fn sync(&mut self) -> eyre::Result<()> {
        self.gamepad.emit(&[*SynchronizationEvent::new(SynchronizationCode::SYN_REPORT, 0)])?;
        self.motion_sensors.emit(&[*SynchronizationEvent::new(SynchronizationCode::SYN_REPORT, 0)])?;

        Ok(())
    }
}