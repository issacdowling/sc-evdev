use std::sync::{mpsc, Arc, Mutex};
use std::sync::mpsc::{Sender, TryRecvError};
use std::time::SystemTime;
use bitflags::{bitflags, bitflags_match};
use bytemuck::{Pod, Zeroable};
use color_eyre::eyre;
use evdev_rs::{AbsInfo, DeviceWrapper, EnableCodeData, InputEvent, TimeVal, UInputDevice, UninitDevice};
use evdev_rs::enums::{BusType, EventCode, EV_KEY, EV_ABS, EV_SYN, EV_MSC};
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

fn event_time_now() -> eyre::Result<TimeVal> {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
    Ok(TimeVal::new(now.as_secs() as i64, now.subsec_micros() as i64))
}

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

        // Maybe enable gyro?

        // controller.send_command_with_payload(Command::Unknown3, &[
        //     0x00, 0x01, 0x02, 0x03,
        //     0x04, 0x05, 0x06, 0x07,
        //     0x08, 0x09, 0x0a, 0x0b,
        //     0x0c, 0x0d, 0x0e, 0x0f,
        // ]).unwrap();
        // controller.send_command_with_payload(Command::SetSettings, &[
        //     0x30, 0x18, 0x00,
        //     0x07, 0x07, 0x00,
        //     0x08, 0x07, 0x00,
        //     0x31, 0x02, 0x00,
        //     0x52, 0x03, 0x00,
        // ]).unwrap();
        controller.send_command_with_payload(Command::SetSettings, &[
            0x18, 0x00, 0x00,
            0x2e, 0x00, 0x00,
            0x34, 0xff, 0xff,
            0x35, 0xff, 0xff,
            0x2e, 0x00, 0x00,
        ]).unwrap();

        // controller.send_command_with_payload(Command::Unknown2, &[
        //     0x01, 0x02,
        // ]).unwrap();
        // controller.send_command_with_payload(Command::Unknown1, &[
        //     0x01, 0x20,
        // ]).unwrap();
        // controller.send_command_with_payload(Command::SetSettings, &[
        //     0x22, 0x64, 0x00,
        // ]).unwrap();
        // controller.send_command_with_payload(Command::SetSettings, &[
        //     0x23, 0x50, 0x00,
        // ]).unwrap();
        // controller.send_command_with_payload(Command::Unknown3, &[
        //     0xff, 0xff, 0xff, 0xff,
        //     0x03, 0x09, 0x05, 0xff,
        //     0xff, 0xff, 0xff, 0xff,
        //     0xff, 0xff, 0xff, 0xff,
        // ]).unwrap();
        //
        // controller.send_command_with_payload(Command::SetSettings, &[
        //     0x30, 0x00, 0x00,
        //     0x07, 0x07, 0x00,
        //     0x08, 0x07, 0x00,
        //     0x31, 0x02, 0x00,
        //     0x52, 0x03, 0x00,
        // ]).unwrap();
        // controller.send_command_with_payload(Command::SetSettings, &[
        //     0x18, 0x00, 0x00,
        //     0x34, 0x0a, 0x00,
        //     0x35, 0x0a, 0x00,
        //     0x2e, 0x00, 0x00,
        //     0x2e, 0x00, 0x00,
        // ]).unwrap();


        // controller.send_command_with_payload(Command::SetSettings, &[
        //     0x34, 0xff, 0xff,
        //     0x35, 0xff, 0xff,
        // ]).unwrap();



        // controller.send_command_with_payload(Command::SetSettings, &[
        //     0x18, 0x00, 0x00,
        //     0x34, 0x0a, 0x00,
        //     0x35, 0x0a, 0x00,
        //     0x2e, 0x00, 0x00,
        //     0x2e, 0x00, 0x00, 0x00,
        // ]).unwrap();
        //
        // controller.send_command_with_payload(Command::SetSettings, &[
        //     0x30, 0x00, 0x00,
        //     0x07, 0x07, 0x00,
        //     0x08, 0x07, 0x00,
        //     0x31, 0x02, 0x00,
        //     0x52, 0x03, 0x00, 0x00,
        // ]).unwrap();
        // controller.send_command_with_payload(Command::SetSettings, &[
        //     0x30, 0x00, 0x00,
        //     0x07, 0x07, 0x00,
        //     0x08, 0x07, 0x00,
        //     0x31, 0x02, 0x00,
        //     0x52, 0x03, 0x00, 0x00,
        // ]).unwrap();

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

                let left_joystick_x = i16::from_le_bytes(buf[10..12].try_into().unwrap());
                let left_joystick_y = i16::from_le_bytes(buf[12..14].try_into().unwrap());
                let right_joystick_x = i16::from_le_bytes(buf[14..16].try_into().unwrap());
                let right_joystick_y = i16::from_le_bytes(buf[16..18].try_into().unwrap());

                virtual_controller.send_joystick_events(left_joystick_x, left_joystick_y, right_joystick_x, right_joystick_y).unwrap();

                let left_pad_x = i16::from_le_bytes(buf[18..20].try_into().unwrap());
                let left_pad_y = i16::from_le_bytes(buf[20..22].try_into().unwrap());
                let left_pad_pressure = i16::from_le_bytes(buf[22..24].try_into().unwrap());

                let right_pad_x = i16::from_le_bytes(buf[24..26].try_into().unwrap());
                let right_pad_y = i16::from_le_bytes(buf[26..28].try_into().unwrap());
                let right_pad_pressure = i16::from_le_bytes(buf[28..30].try_into().unwrap());

                // Not sure if this is actually the accelerometer.
                let accel_x = i16::from_le_bytes(buf[30..32].try_into().unwrap());
                let accel_y = i16::from_le_bytes(buf[32..34].try_into().unwrap());
                let accel_z = i16::from_le_bytes(buf[34..36].try_into().unwrap());
                // println!("Accel: {:?} {:?} {:?}", accel_x, accel_y, accel_z);

                let gyro_x = i16::from_le_bytes(buf[36..38].try_into().unwrap());
                let gyro_y = i16::from_le_bytes(buf[38..40].try_into().unwrap());
                let gyro_z = i16::from_le_bytes(buf[40..42].try_into().unwrap());

                // Copied from https://github.com/torvalds/linux/blob/5d6919055dec134de3c40167a490f33c74c12581/drivers/hid/hid-steam.c#L1686
                virtual_controller.increment_motion_sensor_timestamp(4000);

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
    gamepad_init: UninitDevice,
    gamepad: UInputDevice,
    motion_sensors_init: UninitDevice,
    motion_sensors: UInputDevice,
    motion_sensor_timestamp_us: i32,
}

impl VirtualController {
    pub fn new(device_info: &DeviceInfo) -> eyre::Result<Self> {
        let gamepad_init = UninitDevice::new().unwrap();
        gamepad_init.set_name(&format!("Steam Controller (evdev wrapper for {:?})", device_info.path()));
        gamepad_init.set_bustype(device_info.bus_type() as u16);
        gamepad_init.set_vendor_id(device_info.vendor_id());
        gamepad_init.set_product_id(device_info.product_id());
        gamepad_init.set_uniq(device_info.serial_number().unwrap());
        gamepad_init.set_version(device_info.release_number());

        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_TR2))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_TL2))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_TR))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_TL))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_SOUTH))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_EAST))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_WEST))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_NORTH))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_0))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_1))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_2))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_3))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_4))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_5))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::KEY_BRIGHTNESSDOWN))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::KEY_BRIGHTNESSUP))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_DPAD_UP))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_DPAD_RIGHT))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_DPAD_LEFT))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_DPAD_DOWN))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_SELECT))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_BASE))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_MODE))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_START))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_THUMBR))?;
        gamepad_init.enable(EventCode::EV_KEY(EV_KEY::BTN_THUMBL))?;

        let abs_info = AbsInfo {
            value: 0,
            minimum: -32767,
            maximum: 32767,
            fuzz: 0,
            flat: 0,
            resolution: 6553,
        };
        gamepad_init.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_X), Some(EnableCodeData::AbsInfo(abs_info.clone())))?;
        gamepad_init.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_Y), Some(EnableCodeData::AbsInfo(abs_info.clone())))?;
        gamepad_init.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_RX), Some(EnableCodeData::AbsInfo(abs_info.clone())))?;
        gamepad_init.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_RY), Some(EnableCodeData::AbsInfo(abs_info.clone())))?;

        let abs_info = AbsInfo {
            value: 0,
            minimum: -32767,
            maximum: 32767,
            fuzz: 256,
            flat: 0,
            resolution: 0,
        };
        gamepad_init.set_abs_info(&EventCode::EV_ABS(EV_ABS::ABS_HAT0X), &abs_info);
        gamepad_init.set_abs_info(&EventCode::EV_ABS(EV_ABS::ABS_HAT0Y), &abs_info);

        let abs_info = AbsInfo {
            value: 0,
            minimum: -32767,
            maximum: 32767,
            fuzz: 256,
            flat: 0,
            resolution: 1638,
        };
        gamepad_init.set_abs_info(&EventCode::EV_ABS(EV_ABS::ABS_HAT1X), &abs_info);
        gamepad_init.set_abs_info(&EventCode::EV_ABS(EV_ABS::ABS_HAT1Y), &abs_info);

        let abs_info = AbsInfo {
            value: 0,
            minimum: 0,
            maximum: 32767,
            fuzz: 0,
            flat: 0,
            resolution: 5461,
        };
        gamepad_init.set_abs_info(&EventCode::EV_ABS(EV_ABS::ABS_HAT2X), &abs_info);
        gamepad_init.set_abs_info(&EventCode::EV_ABS(EV_ABS::ABS_HAT2Y), &abs_info);

        let gamepad = UInputDevice::create_from_device(&gamepad_init)?;

        let motion_sensors_init = UninitDevice::new().unwrap();
        motion_sensors_init.set_name(&format!("Steam Controller (Motion Sensors) (evdev wrapper for {:?})", device_info.path()));
        motion_sensors_init.set_bustype(device_info.bus_type() as u16);
        motion_sensors_init.set_vendor_id(device_info.vendor_id());
        motion_sensors_init.set_product_id(device_info.product_id());
        motion_sensors_init.set_uniq(device_info.serial_number().unwrap());
        motion_sensors_init.set_version(device_info.release_number());

        motion_sensors_init.enable_event_code(&EventCode::EV_MSC(EV_MSC::MSC_TIMESTAMP), None)?;

        let accel_abs_info = AbsInfo {
            value: 0,
            minimum: -32768,
            maximum: 32768,
            fuzz: 32,
            flat: 0,
            resolution: 16384,
        };
        motion_sensors_init.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_X), Some(EnableCodeData::AbsInfo(accel_abs_info)))?;
        motion_sensors_init.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_Y), Some(EnableCodeData::AbsInfo(accel_abs_info)))?;
        motion_sensors_init.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_Z), Some(EnableCodeData::AbsInfo(accel_abs_info)))?;

        let gyro_abs_info = AbsInfo {
            value: 0,
            minimum: -32768,
            maximum: 32768,
            fuzz: 1,
            flat: 0,
            resolution: 16,
        };
        motion_sensors_init.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_RX), Some(EnableCodeData::AbsInfo(gyro_abs_info)))?;
        motion_sensors_init.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_RY), Some(EnableCodeData::AbsInfo(gyro_abs_info)))?;
        motion_sensors_init.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_RZ), Some(EnableCodeData::AbsInfo(gyro_abs_info)))?;

        let motion_sensors = UInputDevice::create_from_device(&motion_sensors_init)?;

        Ok(
            VirtualController {
                gamepad_init,
                gamepad,
                motion_sensors_init,
                motion_sensors,
                motion_sensor_timestamp_us: 0,
            }
        )
    }

    pub fn send_button_events(&self, buttons: Buttons, pressed: bool) -> eyre::Result<()> {
        let time = event_time_now()?;

        let event = bitflags_match!(buttons, {
            Buttons::BTN_A => Some(EV_KEY::BTN_SOUTH),
            Buttons::BTN_B => Some(EV_KEY::BTN_EAST),
            Buttons::BTN_X => Some(EV_KEY::BTN_WEST),
            Buttons::BTN_Y => Some(EV_KEY::BTN_NORTH),
            Buttons::BTN_R1 => Some(EV_KEY::BTN_TR),
            Buttons::BTN_L1 => Some(EV_KEY::BTN_TL),
            Buttons::BTN_R2 => Some(EV_KEY::BTN_TR2),
            Buttons::BTN_L2 => Some(EV_KEY::BTN_TL2),
            Buttons::BTN_R4 => Some(EV_KEY::BTN_0),
            Buttons::BTN_L4 => Some(EV_KEY::BTN_1),
            Buttons::BTN_R5 => Some(EV_KEY::BTN_2),
            Buttons::BTN_L5 => Some(EV_KEY::BTN_3),
            Buttons::BTN_RIGHT_PAD_CLICK => Some(EV_KEY::BTN_4),
            Buttons::BTN_LEFT_PAD_CLICK => Some(EV_KEY::BTN_5),
            Buttons::BTN_GRIPR => Some(EV_KEY::KEY_BRIGHTNESSDOWN),
            Buttons::BTN_GRIPL => Some(EV_KEY::KEY_BRIGHTNESSUP),
            Buttons::BTN_THUMBL => Some(EV_KEY::BTN_THUMBL),
            Buttons::BTN_THUMBR => Some(EV_KEY::BTN_THUMBR),
            Buttons::BTN_DPAD_UP => Some(EV_KEY::BTN_DPAD_UP),
            Buttons::BTN_DPAD_DOWN => Some(EV_KEY::BTN_DPAD_DOWN),
            Buttons::BTN_DPAD_LEFT => Some(EV_KEY::BTN_DPAD_LEFT),
            Buttons::BTN_DPAD_RIGHT => Some(EV_KEY::BTN_DPAD_RIGHT),
            Buttons::BTN_START => Some(EV_KEY::BTN_START),
            Buttons::BTN_SELECT => Some(EV_KEY::BTN_SELECT),
            Buttons::BTN_STEAM => Some(EV_KEY::BTN_MODE),
            Buttons::BTN_QUICK_ACCESS => Some(EV_KEY::BTN_BASE),
            _ => None,
        });

        if let Some(event) = event {
            let value = if pressed { 1 } else { 0 };

            self.gamepad.write_event(&InputEvent {
                time,
                event_code: EventCode::EV_KEY(event),
                value,
            })?;
        }

        Ok(())
    }

    pub fn send_joystick_events(&self, left_joystick_x: i16, left_joystick_y: i16, right_joystick_x: i16, right_joystick_y: i16) -> eyre::Result<()> {
        let time = event_time_now()?;

        self.gamepad.write_event(&InputEvent {
            time,
            event_code: EventCode::EV_ABS(EV_ABS::ABS_X),
            value: left_joystick_x as i32,
        })?;
        self.gamepad.write_event(&InputEvent {
            time,
            event_code: EventCode::EV_ABS(EV_ABS::ABS_Y),
            value: -left_joystick_y as i32,
        })?;
        self.gamepad.write_event(&InputEvent {
            time,
            event_code: EventCode::EV_ABS(EV_ABS::ABS_RX),
            value: right_joystick_x as i32,
        })?;
        self.gamepad.write_event(&InputEvent {
            time,
            event_code: EventCode::EV_ABS(EV_ABS::ABS_RY),
            value: -right_joystick_y as i32,
        })?;

        Ok(())
    }

    pub fn increment_motion_sensor_timestamp(&mut self, delta_us: i32) {
        if let Some(motion_sensor_timestamp_us) = self.motion_sensor_timestamp_us.checked_add(delta_us) {
            self.motion_sensor_timestamp_us = motion_sensor_timestamp_us;
        } else {
            // Hopefully doing this won't break anything, I have no idea what else to do.
            self.motion_sensor_timestamp_us = 0;
        }
    }

    pub fn send_motion_sensor_events(
        &self,
        gyro_x: i16,
        gyro_y: i16,
        gyro_z: i16,
    ) -> eyre::Result<()> {
        let time = event_time_now()?;

        self.motion_sensors.write_event(&InputEvent {
            time,
            event_code: EventCode::EV_MSC(EV_MSC::MSC_TIMESTAMP),
            value: self.motion_sensor_timestamp_us,
        })?;

        self.motion_sensors.write_event(&InputEvent {
            time,
            event_code: EventCode::EV_ABS(EV_ABS::ABS_RX),
            value: gyro_x as i32,
        })?;
        self.motion_sensors.write_event(&InputEvent {
            time,
            event_code: EventCode::EV_ABS(EV_ABS::ABS_RY),
            value: gyro_y as i32,
        })?;
        self.motion_sensors.write_event(&InputEvent {
            time,
            event_code: EventCode::EV_ABS(EV_ABS::ABS_RZ),
            value: gyro_z as i32,
        })?;

        Ok(())
    }

    pub fn sync(&self) -> eyre::Result<()> {
        let time = event_time_now()?;

        self.gamepad.write_event(&InputEvent {
            time,
            event_code: EventCode::EV_SYN(EV_SYN::SYN_REPORT),
            value: 0,
        })?;

        Ok(())
    }
}