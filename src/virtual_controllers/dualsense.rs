use std::fs::File;
use std::sync::{mpsc, Arc, Mutex};
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};
use bitflags::bitflags;
use color_eyre::eyre;
use uhid_virt::{Bus, CreateParams, OutputEvent, UHIDDevice};
use crate::virtual_controllers::JoystickXY;

const REPORT_DESCRIPTOR: [u8; 273] = [
    0x05, 0x01,       // Usage Page (Generic Desktop Ctrls)
    0x09, 0x05,       // Usage (Game Pad)
    0xA1, 0x01,       // Collection (Application)
    0x85, 0x01,       //   Report ID (1)
    0x09, 0x30,       //   Usage (X)
    0x09, 0x31,       //   Usage (Y)
    0x09, 0x32,       //   Usage (Z)
    0x09, 0x35,       //   Usage (Rz)
    0x09, 0x33,       //   Usage (Rx)
    0x09, 0x34,       //   Usage (Ry)
    0x15, 0x00,       //   Logical Minimum (0)
    0x26, 0xFF, 0x00, //   Logical Maximum (255)
    0x75, 0x08,       //   Report Size (8)
    0x95, 0x06,       //   Report Count (6)
    0x81, 0x02,       //   Input (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position)
    0x06, 0x00, 0xFF, //   Usage Page (Vendor Defined 0xFF00)
    0x09, 0x20,       //   Usage (0x20)
    0x95, 0x01,       //   Report Count (1)
    0x81, 0x02,       //   Input (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position)
    0x05, 0x01,       //   Usage Page (Generic Desktop Ctrls)
    0x09, 0x39,       //   Usage (Hat switch)
    0x15, 0x00,       //   Logical Minimum (0)
    0x25, 0x07,       //   Logical Maximum (7)
    0x35, 0x00,       //   Physical Minimum (0)
    0x46, 0x3B, 0x01, //   Physical Maximum (315)
    0x65, 0x14,       //   Unit (System: English Rotation, Length: Centimeter)
    0x75, 0x04,       //   Report Size (4)
    0x95, 0x01,       //   Report Count (1)
    0x81, 0x42,       //   Input (Data,Var,Abs,No Wrap,Linear,Preferred State,Null State)
    0x65, 0x00,       //   Unit (None)
    0x05, 0x09,       //   Usage Page (Button)
    0x19, 0x01,       //   Usage Minimum (0x01)
    0x29, 0x0F,       //   Usage Maximum (0x0F)
    0x15, 0x00,       //   Logical Minimum (0)
    0x25, 0x01,       //   Logical Maximum (1)
    0x75, 0x01,       //   Report Size (1)
    0x95, 0x0F,       //   Report Count (15)
    0x81, 0x02,       //   Input (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position)
    0x06, 0x00, 0xFF, //   Usage Page (Vendor Defined 0xFF00)
    0x09, 0x21,       //   Usage (0x21)
    0x95, 0x0D,       //   Report Count (13)
    0x81, 0x02,       //   Input (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position)
    0x06, 0x00, 0xFF, //   Usage Page (Vendor Defined 0xFF00)
    0x09, 0x22,       //   Usage (0x22)
    0x15, 0x00,       //   Logical Minimum (0)
    0x26, 0xFF, 0x00, //   Logical Maximum (255)
    0x75, 0x08,       //   Report Size (8)
    0x95, 0x34,       //   Report Count (52)
    0x81, 0x02,       //   Input (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position)
    0x85, 0x02,       //   Report ID (2)
    0x09, 0x23,       //   Usage (0x23)
    0x95, 0x2F,       //   Report Count (47)
    0x91, 0x02,       //   Output (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0x05,       //   Report ID (5)
    0x09, 0x33,       //   Usage (0x33)
    0x95, 0x28,       //   Report Count (40)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0x08,       //   Report ID (8)
    0x09, 0x34,       //   Usage (0x34)
    0x95, 0x2F,       //   Report Count (47)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0x09,       //   Report ID (9)
    0x09, 0x24,       //   Usage (0x24)
    0x95, 0x13,       //   Report Count (19)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0x0A,       //   Report ID (10)
    0x09, 0x25,       //   Usage (0x25)
    0x95, 0x1A,       //   Report Count (26)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0x20,       //   Report ID (32)
    0x09, 0x26,       //   Usage (0x26)
    0x95, 0x3F,       //   Report Count (63)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0x21,       //   Report ID (33)
    0x09, 0x27,       //   Usage (0x27)
    0x95, 0x04,       //   Report Count (4)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0x22,       //   Report ID (34)
    0x09, 0x40,       //   Usage (0x40)
    0x95, 0x3F,       //   Report Count (63)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0x80,       //   Report ID (-128)
    0x09, 0x28,       //   Usage (0x28)
    0x95, 0x3F,       //   Report Count (63)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0x81,       //   Report ID (-127)
    0x09, 0x29,       //   Usage (0x29)
    0x95, 0x3F,       //   Report Count (63)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0x82,       //   Report ID (-126)
    0x09, 0x2A,       //   Usage (0x2A)
    0x95, 0x09,       //   Report Count (9)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0x83,       //   Report ID (-125)
    0x09, 0x2B,       //   Usage (0x2B)
    0x95, 0x3F,       //   Report Count (63)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0x84,       //   Report ID (-124)
    0x09, 0x2C,       //   Usage (0x2C)
    0x95, 0x3F,       //   Report Count (63)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0x85,       //   Report ID (-123)
    0x09, 0x2D,       //   Usage (0x2D)
    0x95, 0x02,       //   Report Count (2)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0xA0,       //   Report ID (-96)
    0x09, 0x2E,       //   Usage (0x2E)
    0x95, 0x01,       //   Report Count (1)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0xE0,       //   Report ID (-32)
    0x09, 0x2F,       //   Usage (0x2F)
    0x95, 0x3F,       //   Report Count (63)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0xF0,       //   Report ID (-16)
    0x09, 0x30,       //   Usage (0x30)
    0x95, 0x3F,       //   Report Count (63)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0xF1,       //   Report ID (-15)
    0x09, 0x31,       //   Usage (0x31)
    0x95, 0x3F,       //   Report Count (63)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0xF2,       //   Report ID (-14)
    0x09, 0x32,       //   Usage (0x32)
    0x95, 0x0F,       //   Report Count (15)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0xF4,       //   Report ID (-12)
    0x09, 0x35,       //   Usage (0x35)
    0x95, 0x3F,       //   Report Count (63)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0x85, 0xF5,       //   Report ID (-11)
    0x09, 0x36,       //   Usage (0x36)
    0x95, 0x03,       //   Report Count (3)
    0xB1, 0x02,       //   Feature (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position,Non-volatile)
    0xC0,             // End Collection
];

const CALIBRATION_INFO: [u8; 41] = [
    0x05,
    0x00, // gyro_pitch_bias
    0x00,
    0x00, // gyro_yaw_bias
    0x00,
    0x00, // gyro_roll_bias
    0x00,
    0x10, // gyro_pitch_plus
    0x27,
    0xF0, // gyro_pitch_minus
    0xD8,
    0x10, // gyro_yaw_plus
    0x27,
    0xF0, // gyro_yaw_minus
    0xD8,
    0x10, // gyro_roll_plus
    0x27,
    0xF0, // gyro_roll_minus
    0xD8,
    0xF4, // gyro_speed_plus
    0x01,
    0xF4, // gyro_speed_minus
    0x01,
    0x10, // acc_x_plus
    0x27,
    0xF0, // acc_x_minus
    0xD8,
    0x10, // acc_y_plus
    0x27,
    0xF0, // acc_y_minus
    0xD8,
    0x10, // acc_z_plus
    0x27,
    0xF0, // acc_z_minus
    0xD8, 0x0B, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const FIRMWARE_INFO: [u8; 64] = [
    0x20, 0x4A, 0x75, 0x6E, 0x20, 0x31, 0x39, 0x20, 0x32, 0x30, 0x32, 0x33, 0x31, 0x34, 0x3A, 0x34,
    0x37, 0x3A, 0x33, 0x34, 0x03, 0x00, 0x44, 0x00, 0x08, 0x02, 0x00, 0x01, 0x36, 0x00, 0x00, 0x01,
    0xC1, 0xC8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x54, 0x01, 0x00, 0x00,
    0x14, 0x00, 0x00, 0x00, 0x0B, 0x00, 0x01, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const PAIRING_INFO: [u8; 20] = [
    0x09, 0x74, 0xE7, 0xD6, 0x3A, 0x53, 0x35, 0x08, 0x25, 0x00,
    0x1E, 0x00, 0xEE, 0x74, 0xD0, 0xBC, 0x00, 0x00, 0x00, 0x00,
];

const DUALSENSE_TOUCHPAD_WIDTH: u16 = 1920;
const DUALSENSE_TOUCHPAD_HEIGHT: u16 = 1080;

bitflags! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DualsenseButtons: u32 {
        // DPad is stored here but will be transformed.
        const DPAD_UP = 0x01 << 24;
        const DPAD_DOWN = 0x02 << 24;
        const DPAD_LEFT = 0x04 << 24;
        const DPAD_RIGHT = 0x08 << 24;

        const SQUARE = 0x10 << 24;
        const CROSS = 0x20 << 24;
        const CIRCLE = 0x40 << 24;
        const TRIANGLE = 0x80 << 24;
        const L1 = 0x01 << 16;
        const R1 = 0x01 << 17;
        const L2 = 0x01 << 18;
        const R2 = 0x01 << 19;
        const CREATE = 0x01 << 20;
        const OPTIONS = 0x01 << 21;
        const L3 = 0x01 << 22;
        const R3 = 0x01 << 23;
        const PS = 0x01 << 8;
        const TOUCHPAD = 0x01 << 9;
        const MUTE = 0x01 << 10;
    }
}

impl DualsenseButtons {
    pub fn as_button_bytes(&self) -> [u8; 4] {
        let value = self.bits();

        let mut bytes = [0; 4];
        bytes[0] = (value >> 24) as u8;
        bytes[1] = (value >> 16) as u8;
        bytes[2] = (value >> 8) as u8;
        bytes[3] = value as u8;

        // Transform DPad to expected format.
        let face_buttons_value = bytes[0] & 0xF0;
        let mut dpad: u8 = 0x08;
        if self.contains(DualsenseButtons::DPAD_UP) && !self.contains(DualsenseButtons::DPAD_LEFT) {
            dpad = 0x00;
            if self.contains(DualsenseButtons::DPAD_RIGHT) {
                dpad += 0x01;
            }
        } else if self.contains(DualsenseButtons::DPAD_RIGHT) {
            dpad = 0x02;
            if self.contains(DualsenseButtons::DPAD_DOWN) {
                dpad += 0x01;
            }
        } else if self.contains(DualsenseButtons::DPAD_DOWN) {
            dpad = 0x04;
            if self.contains(DualsenseButtons::DPAD_LEFT) {
                dpad += 0x01;
            }
        } else if self.contains(DualsenseButtons::DPAD_LEFT) {
            dpad = 0x06;
            if self.contains(DualsenseButtons::DPAD_UP) {
                dpad += 0x01;
            }
        }
        bytes[0] = face_buttons_value | dpad;

        bytes
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
enum MappedPad {
    #[default]
    None,
    Left,
    Right,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
struct TouchPoint {
    // If false, only the id is required to be valid.
    active: bool,
    // Limited to 7 bits, max value of 127, wraps around to 0.
    id: u8,
    x: u16,
    y: u16,
    mapped_pad: MappedPad,
}

impl TouchPoint {
    pub fn new() -> Self {
        TouchPoint::default()
    }

    pub fn next_id(&mut self) {
        self.id = (self.id + 1) % 128;
    }

    pub fn as_bytes(&self) -> [u8; 4] {
        let mut bytes = [0; 4];

        let active_bit = if self.active { 0x00 } else { 0x80 };
        bytes[0] = active_bit | self.id;

        let x = self.x & 0x0FFF;
        let x_high = ((x >> 8) & 0x0F) as u8;
        let x_low = (x & 0xFF) as u8;

        let y = self.y & 0x0FFF;
        let y_low = (y & 0x000F) as u8;
        let y_high = ((y & 0xFFF0) >> 4) as u8;

        bytes[1] = x_low;
        bytes[2] = x_high | (y_low << 4);
        bytes[3] = y_high;

        bytes
    }
}

pub struct VirtualDualSenseController {
    uhid_device: Arc<Mutex<UHIDDevice<File>>>,
    read_handle: Option<JoinHandle<()>>,
    read_tx: mpsc::Sender<bool>,
    sequence_number: u8,
    accel: (i16, i16, i16),
    gyro: (i16, i16, i16),
    buttons: DualsenseButtons,
    left_stick: (u8, u8),
    right_stick: (u8, u8),
    touch_points: [TouchPoint; 2],
}

impl VirtualDualSenseController {
    pub fn new() -> eyre::Result<VirtualDualSenseController> {
        let rd_data = REPORT_DESCRIPTOR.to_vec();
        let create_params = CreateParams {
            name: "Wireless Controller".to_string(),
            phys: "".to_string(),
            uniq: "".to_string(),
            bus: Bus::USB,
            vendor: 0x054c,
            product: 0x0ce6,
            version: 0x100,
            country: 0,
            rd_data,
        };

        let uhid_device = Arc::new(Mutex::new(UHIDDevice::create(create_params)?));

        let (tx, rx) = mpsc::channel::<bool>();

        let read_device = uhid_device.clone();
        let read_handle = thread::spawn(move || {
            loop {
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

                let mut device = read_device.lock().unwrap();
                let res = device.read();
                if let Ok(event) = res {
                    match event {
                        OutputEvent::Start { dev_flags } => {
                            println!("Start event received with flags");
                        }
                        OutputEvent::Stop => {
                            println!("Stop event received");
                        }
                        OutputEvent::Open => {
                            println!("Open event received");
                        }
                        OutputEvent::Close => {
                            println!("Close event received");
                        }
                        OutputEvent::Output { data } => {
                            // println!("Output event received with data: {:?}", data);
                        }
                        OutputEvent::GetReport { id, report_number, report_type } => {
                            println!("get_report: (id: {id:#?}, report_number: {report_number:#?}, report_type: {report_type:#?})");

                            if report_number == 0x05 {
                                device.write_get_report_reply(id, 0, CALIBRATION_INFO.to_vec()).unwrap();
                            } else if report_number == 0x09 {
                                device.write_get_report_reply(id, 0, PAIRING_INFO.to_vec()).unwrap();
                            } else if report_number == 0x20 {
                                device.write_get_report_reply(id, 0, FIRMWARE_INFO.to_vec()).unwrap();
                            }
                        }
                        OutputEvent::SetReport { id, report_number, report_type, data } => {
                            println!("set_report: (id: {id:#?}, report_number: {report_number:#?}, report_type: {report_type:#?}, data: {data:#?})");
                        }
                    }
                }
            }
        });

        Ok(
            VirtualDualSenseController {
                uhid_device,
                read_handle: Some(read_handle),
                read_tx: tx,
                sequence_number: 0,
                accel: (0, 0, 0),
                gyro: (0, 0, 0),
                buttons: DualsenseButtons::empty(),
                left_stick: (0, 0),
                right_stick: (0, 0),
                touch_points: [TouchPoint::new(); 2],
            }
        )
    }

    pub fn update_buttons(&mut self, buttons: DualsenseButtons, pressed: bool) {
        if pressed {
            self.buttons.insert(buttons);
        } else {
            self.buttons.remove(buttons);
        }
    }

    pub fn set_sticks(&mut self, left_stick: JoystickXY, right_stick: JoystickXY) {
        self.left_stick = left_stick.as_dualsense_stick_data();
        self.right_stick = right_stick.as_dualsense_stick_data();
    }

    fn get_best_touch_point(&mut self, mapped_pad: MappedPad, active: bool) -> &mut TouchPoint {
        if self.touch_points[0].mapped_pad == mapped_pad && self.touch_points[0].active == active {
            return &mut self.touch_points[0];
        }
        if self.touch_points[1].mapped_pad == mapped_pad && self.touch_points[1].active == active {
            return &mut self.touch_points[1];
        }
        if mapped_pad == MappedPad::Left {
            if !self.touch_points[0].active && active {
                return &mut self.touch_points[0];
            }
        } else if mapped_pad == MappedPad::Right {
            if !self.touch_points[1].active && active {
                return &mut self.touch_points[1];
            }
        }
        if self.touch_points[1].active != active && self.touch_points[0].active == active {
            return &mut self.touch_points[0];
        }
        &mut self.touch_points[1]
    }

    pub fn update_touchpad(
        &mut self,
        pad_x: i16,
        pad_y: i16,
        pad_pressure: i16,
        pad: u8,
    ) {
        let mapped_pad = if pad != 0 {
            MappedPad::Right
        } else {
            MappedPad::Left
        };

        let touch_point = self.get_best_touch_point(mapped_pad, true);

        if pad_x == 0 && pad_y == 0 && pad_pressure == 0 {
            touch_point.active = false;
            touch_point.mapped_pad = MappedPad::None;
            return;
        }
        let pad_pos = JoystickXY::from_steam_controller_stick_data((pad_x, pad_y))
            .to_linear01();

        let pad_x = (pad_pos.x * DUALSENSE_TOUCHPAD_WIDTH as f32).ceil() as u16;
        let pad_y = (pad_pos.y * DUALSENSE_TOUCHPAD_HEIGHT as f32).ceil() as u16;

        if !touch_point.active {
            touch_point.next_id();
            touch_point.active = true;
            touch_point.mapped_pad = mapped_pad;
        }
        touch_point.x = pad_x;
        touch_point.y = pad_y;
    }

    pub fn update_motion_sensors(
        &mut self,
        accel_x: i16,
        accel_y: i16,
        accel_z: i16,
        gyro_x: i16,
        gyro_y: i16,
        gyro_z: i16,
    ) {

        self.accel = (accel_x, accel_y, accel_z);
        self.gyro = (gyro_x, gyro_y, gyro_z);
    }

    pub fn update_inputs(&mut self) -> eyre::Result<()> {
        let button_bytes = self.buttons.as_button_bytes();

        let gyro_x_bytes = self.gyro.0.to_le_bytes();
        let gyro_y_bytes = self.gyro.1.to_le_bytes();
        let gyro_z_bytes = self.gyro.2.to_le_bytes();

        let accel_x_bytes = self.accel.0.to_le_bytes();
        let accel_y_bytes = self.accel.1.to_le_bytes();
        let accel_z_bytes = self.accel.2.to_le_bytes();

        let touch_point_0_bytes = self.touch_points[0].as_bytes();
        let touch_point_1_bytes = self.touch_points[1].as_bytes();

        let sensors_timestamp_bytes = ((SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() / 333) as u32).to_le_bytes();
        {
            let mut device = self.uhid_device.lock()
                .expect("Failed to lock uhid device");

            device.write(&[
                0x01, // Report ID
                self.left_stick.0, self.left_stick.1, // Left Stick
                self.right_stick.0, self.right_stick.1, // Right Stick
                0x00, 0x00, // L2, R2
                self.sequence_number, // Sequence Number
                button_bytes[0], button_bytes[1], button_bytes[2], button_bytes[3], // Buttons
                0xac, 0x0a, 0xaf, 0x14, // Reserved 1
                gyro_x_bytes[0], gyro_x_bytes[1], // Gryo X
                gyro_y_bytes[0], gyro_y_bytes[1], // Gyro Y
                gyro_z_bytes[0], gyro_z_bytes[1], // Gyro Z
                accel_x_bytes[0], accel_x_bytes[1], // Accel X
                accel_y_bytes[0], accel_y_bytes[1], // Accel Y
                accel_z_bytes[0], accel_z_bytes[1], // Accel Z
                sensors_timestamp_bytes[0], sensors_timestamp_bytes[1], sensors_timestamp_bytes[2], sensors_timestamp_bytes[3], // Timestamp
                0x1b, // Reserved 2
                touch_point_0_bytes[0], touch_point_0_bytes[1], touch_point_0_bytes[2], touch_point_0_bytes[3], // Touchpoint 0
                touch_point_1_bytes[0], touch_point_1_bytes[1], touch_point_1_bytes[2], touch_point_1_bytes[3], // Touchpoint 1
                0xbd, // Reserved 3,
                0x09, // R2 adaptive trigger
                0x09, // L2 adaptive trigger
                0x00, 0x00, 0x00, 0x00, 0x00, 0x92, 0xa0, 0xe8, // Reserved 4,
                0xae, // Battery Charge
                0x29, // Battery status
                0x0c, // Battery 2
                0x00, 0xb0, 0x7e, 0xc8, 0x76, 0xf8, 0xcc, 0xa2, 0x2b // Reserved 6
            ])?;
        }

        if let Some(sequence_number) = self.sequence_number.checked_add(1) {
            self.sequence_number = sequence_number;
        } else {
            self.sequence_number = 0;
        };

        Ok(())
    }
}

impl Drop for VirtualDualSenseController {
    fn drop(&mut self) {
        self.read_tx.send(true).unwrap();
        if let Some(read_handle) = self.read_handle.take() {
            read_handle.join().unwrap();
        }
    }
}