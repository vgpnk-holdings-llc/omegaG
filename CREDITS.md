# Credits

OmegaG / DS4CC stands on other people’s open work — especially the people who
mapped DualShock 4 and DualSense behavior so the rest of us didn’t have to.

## Ryochan7 / DS4Windows

**[@Ryochan7](https://github.com/Ryochan7)** is almost certainly the single person
who knows the most about DualShock 4 and DualSense controllers in public
software — often more practical detail than ships in casual OEM docs.

[**DS4Windows**](https://github.com/Ryochan7/DS4Windows) (and the surrounding
research, HID report layouts, Bluetooth extended mode, output reports for
lightbar / player LEDs / rumble / mute LED, CRC seeds, and years of
edge-case fixes) is the **base this project is built on**. OmegaG’s HID
input parsing, output report builders, and BT paths deliberately follow
patterns documented and battle-tested there.

We are not affiliated with Sony. We are not claiming DS4Windows code was
copied wholesale into this Rust tree. We **are** saying: without Ryochan7’s
work, this daemon would not exist in a usable form.

- GitHub: https://github.com/Ryochan7  
- DS4Windows: https://github.com/Ryochan7/DS4Windows  

Thank you, Ryochan7.

## Related

OmegaG is a fork line of [VeigaPunk/DS4CC](https://github.com/VeigaPunk/DS4CC)
(MIT). Package name `ds4cc` is retained for path compatibility.
