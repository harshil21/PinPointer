import paramiko
import time
import sys

# Configuration - customize these
PI_IP = '10.140.80.202'  # Raspberry Pi's IP address
PI_USERNAME = 'harshil'       # Default username
PI_PASSWORD = ''  # Replace with your password (or use key-based auth for security)
SERIAL_DEVICE = '/dev/serial0'  # Serial port on Pi (e.g., /dev/ttyUSB0 for USB adapter)
BAUD_RATE = 115200         # Baud rate for your sensors

def stream_serial_over_ssh():
    # Create SSH client
    ssh = paramiko.SSHClient()
    ssh.set_missing_host_key_policy(paramiko.AutoAddPolicy())
    
    try:
        # Connect to Pi
        ssh.connect(PI_IP, username=PI_USERNAME, password=PI_PASSWORD)
        print("Connected to Raspberry Pi via SSH.")
        
        # Configure serial port (set baud rate, raw mode, etc.)
        config_cmd = f'stty -F {SERIAL_DEVICE} {BAUD_RATE} raw -echo'
        stdin, stdout, stderr = ssh.exec_command(config_cmd)
        if stderr.read():
            print(f"Error configuring serial: {stderr.read().decode()}")
            sys.exit(1)
        print("Serial port configured.")
        
        # Start streaming with cat (reads indefinitely)
        stream_cmd = f'cat {SERIAL_DEVICE}'
        stdin, stdout, stderr = ssh.exec_command(stream_cmd)
        
        # Read and process stream in real-time
        print("Streaming data from Pi's serial port (press Ctrl+C to stop):")
        while True:
            data = stdout.read(1)  # Read byte-by-byte for real-time feel
            if data:
                # Process data here (e.g., decode, analyze, save to file)
                sys.stdout.write(data.decode(errors='ignore'))  # Print raw
                sys.stdout.flush()
            else:
                time.sleep(0.01)  # Small delay to avoid CPU spin
            
    except Exception as e:
        print(f"Error: {e}")
    finally:
        ssh.close()
        print("\nSSH connection closed.")

if __name__ == "__main__":
    stream_serial_over_ssh()