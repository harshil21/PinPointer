import firm_client

FIRM_PORT = "/dev/ttyACM0"  # Update as needed (e.g., "COM8" or "/dev/ttyACM0")

client = firm_client.FIRMClient(FIRM_PORT)
client.start()

client.reboot()