#!/usr/bin/env python3
"""
Helper script to install PySide6 in Binary Ninja's Python environment
"""

import subprocess
import sys
import os

def install_pyside():
    """Install PySide6 using pip"""
    try:
        # Try to install PySide6
        print("Installing PySide6...")
        subprocess.check_call([sys.executable, "-m", "pip", "install", "PySide6"])
        print("PySide6 installed successfully!")
        
        # Test import
        try:
            import PySide6.QtWidgets
            print("PySide6 import test successful!")
            return True
        except ImportError as e:
            print(f"PySide6 import failed: {e}")
            return False
            
    except subprocess.CalledProcessError as e:
        print(f"Failed to install PySide6: {e}")
        
        # Try PySide2 as fallback
        try:
            print("Trying PySide2 as fallback...")
            subprocess.check_call([sys.executable, "-m", "pip", "install", "PySide2"])
            print("PySide2 installed successfully!")
            
            # Test import
            try:
                import PySide2.QtWidgets
                print("PySide2 import test successful!")
                return True
            except ImportError as e:
                print(f"PySide2 import failed: {e}")
                return False
        except subprocess.CalledProcessError as e2:
            print(f"Failed to install PySide2: {e2}")
            return False

def check_installation():
    """Check if PySide6 or PySide2 is available"""
    try:
        import PySide6.QtWidgets
        print("PySide6 is available!")
        return True
    except ImportError:
        pass
    
    try:
        import PySide2.QtWidgets
        print("PySide2 is available!")
        return True
    except ImportError:
        pass
    
    print("Neither PySide6 nor PySide2 is available.")
    return False

if __name__ == "__main__":
    print("Binary Ninja Qt GUI Setup")
    print("=" * 30)
    print(f"Python executable: {sys.executable}")
    print(f"Python version: {sys.version}")
    
    if check_installation():
        print("Qt GUI dependencies are already installed!")
    else:
        print("Installing Qt GUI dependencies...")
        if install_pyside():
            print("Installation successful! The Qt GUI should now work.")
        else:
            print("Installation failed. The plugin will work without GUI features.")
            print("You can manually install PySide6 or PySide2 using:")
            print("  pip install PySide6")
            print("  or")
            print("  pip install PySide2")