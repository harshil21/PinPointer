import spidev

spi = spidev.SpiDev()
spi.open(0, 0)  # Bus 0, CE0 (change to spi.open(0, 1) for CE1)
spi.max_speed_hz = 1000000
spi.mode = 0

# Read version register 0x42 (MSB=0 for read)
resp = spi.xfer2([0x42 & 0x7F, 0x00])  # Send address, dummy byte for response
version = resp[1]
print(f"Version: 0x{version:02X}")  # Should be 0x12

spi.close()

