import board
import digitalio
import time
from adafruit_rfm import rfm9x  # Your modified library

RADIO_FREQ_MHZ = 915.0

# Reset pin setup (same as transmitter)
reset = digitalio.DigitalInOut(board.D17)  # RESET on GPIO17 (physical pin 11)

# Initialize RFM9x (match transmitter settings; baudrate=1MHz for reliability)
rfm = rfm9x.RFM9x(
    RADIO_FREQ_MHZ,
    reset,
    baudrate=1_000_000,  # Match transmitter
    high_power=True,     # Assuming high-power RFM95
    crc=True             # Enable CRC to match default
)
rfm.tx_power = 13                # Not needed for RX, but for consistency
rfm.spreading_factor = 7         # Match transmitter
rfm.signal_bandwidth = 125000    # Match transmitter

# Receiver mode
def receive_message():
    packet = rfm.receive(timeout=10.0)  # Wait up to 10s for a packet
    if packet is not None:
        try:
            message_text = str(packet, "utf-8")  # Decode as UTF-8
        except UnicodeError:
            message_text = "Invalid UTF-8: " + repr(packet)  # Fallback if garbled
        print("Received:", message_text)
        print("RSSI:", rfm.rssi, "dBm")
        print("SNR:", rfm.snr, "dB")
    else:
        print("No packet received (timeout)")

if __name__ == "__main__":
    print("Starting LoRa receiver on 915MHz...")
    while True:
        receive_message()
        time.sleep(0.1)  # Short delay to avoid CPU spin