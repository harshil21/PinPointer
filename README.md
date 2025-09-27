View this project on [CADLAB.io](https://cadlab.io/project/29625). 

# PinPointer

This repository will contain all the components which I intend to use for the payload of my rocket. It will include but won't be limited to:

1) Original KiCAD designs for integrating Quectel's LC29H (DA) and (BS) GPS RTK modules
2) Custom Antenna(s) research, building, and tuning, for both GPS and radio telemetry to receive RTCM correction messages from my base station to the rocket.
3) CAD design for assembly of the antennas and modules into the rocket.
4) Microcontroller code (exact microcontroller TBD) which will communicate with an accelerometer (TBD), the GPS module, and a LoRA module (TBD) to transmit and receive at the 915 MHz ISM band.
5) Openrocket file

Primary goals:
1) Scientific curiosity to compare very accurate GPS altitude with pressure altitude.
2) Package and integrate electronics tightly to minimize cost while at the same time have high performance and portability across different rockets. deliberate design decisions.
3) Pave the way for more experimental flights, e.g. active roll control, rocket landings, etc.

Each section will contain how-to's and thought processes. This data will be free and public to everyone who would also like to do the same and iterate.

🚀📡🛰️
