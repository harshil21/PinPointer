import board
import busio
import digitalio
import time
from adafruit_rfm import rfm9x


RADIO_FREQ_MHZ = 915.0

# SPI setup
spi = busio.SPI(board.SCLK, MOSI=board.MOSI, MISO=board.MISO)
cs = digitalio.DigitalInOut(board.CE0)  # NSS/CS
reset = digitalio.DigitalInOut(board.D17)  # RESET

rfm = rfm9x.RFM9x(spi, cs, reset, 915)  # e.g., 915 MHz for US/AU
rfm.tx_power = 13  # Transmit power in dBm (5-23)
rfm.spreading_factor = 7  # 6-12
rfm.signal_bandwidth = 125000  # Bandwidth in Hz

# Sender mode (run on one Pi)
def send_message():
    message = "Hello from Raspberry Pi!"
    rfm.send(bytes(message, "utf-8"))
    print("Sent:", message)
    time.sleep(5)  # Delay between sends


if __name__ == "__main__":
    while True:
        send_message()