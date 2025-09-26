import RPi.GPIO as GPIO
import time

CS_PIN = 8  # GPIO8/CE0 (change to 7 for CE1 test)
GPIO.setmode(GPIO.BCM)
GPIO.setup(CS_PIN, GPIO.OUT)

try:
    print("Setting CS high (should be ~3.3V)")
    GPIO.output(CS_PIN, GPIO.HIGH)
    time.sleep(5)  # Measure voltage here
    print("Setting CS low (should be ~0V)")
    GPIO.output(CS_PIN, GPIO.LOW)
    time.sleep(5)  # Measure here
    print("Setting CS back high")
    GPIO.output(CS_PIN, GPIO.HIGH)
except KeyboardInterrupt:
    pass
finally:
    GPIO.cleanup()
