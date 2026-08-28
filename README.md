# RL Swapper

A fast and safe in-game item swapper for Rocket League.

## How to get Painted (Colored) Items
If you want an item in a specific paint color (like Titanium White or Crimson), **your Sacrifice item MUST be that color.** 
The Visual item you choose will adopt whatever color your Sacrifice item has equipped in-game.

## False Positives (Antivirus Warnings)
The .exe provided in the Releases tab is compiled using **PyInstaller**. Windows Defender often flags PyInstaller executables as a false positive because the app isn't digitally signed with an expensive developer certificate.

If you don't feel comfortable running the .exe, the entire source code is provided here so you can run it directly yourself.

## How to run from source code
1. Install Python 3.10 or newer from [python.org](https://www.python.org).
2. Download or clone this repository.
3. Open a terminal or command prompt in the folder.
4. Install the required UI library:
   \pip install customtkinter\
5. Run the script:
   \python SwapperGUI.py\

## Notes on Swapping
To perfectly preserve the original file structure and prevent the game from crashing, you cannot swap a long Sacrifice item into a short Visual item. The GUI automatically handles this for you by filtering the "Sacrifice" list to only show items that are safe to use!
