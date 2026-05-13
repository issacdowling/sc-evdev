# sc-evdev

A WIP linux user-space "driver" for the Steam Controller (2026) that allows it to be an actual controller without steam.

Not quite ready for real usage yet.

TODO:
- GUI for configuration.
- System tray icon for quick access to GUI and quick options.
- distro packaging (Fedora, Ubuntu, AUR, Flatpak).
- Proper rumble support.
- Haptic support.

Status Legend

| Icon               | Description                                 |
|--------------------|---------------------------------------------|
| :white_check_mark: | Working                                     |
| :construction:     | Implemented, but not verified to be correct |
| :x:                | Not yet implemented                         |
| :question:         | Not supported                               |


Supported Features for each "driver".

| Feature        | evdev              | Dualsense uhid     |
|----------------|--------------------|--------------------|
| Buttons        | :white_check_mark: | :white_check_mark: |
| Paddle Buttons | :white_check_mark: | :question:         |
| Joysticks      | :white_check_mark: | :white_check_mark: |
| Touchpads      | :white_check_mark: | :white_check_mark: |
| Gyro           | :white_check_mark: | :construction:     |
| Accelerometer  | :white_check_mark: | :construction:     |
| Rumble         | :construction:     | :construction:     |
| Haptics        | :construction:     | :construction:     |