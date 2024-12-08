# Discraft
Playing Minecraft through Discord!

# The Original Idea

I wondered: would it be possible to play Minecraft through Discord? That would need some async knowledge, wouldn't it? And it's quite funny, right?

An original idea I tried to write in C++ but got stuck at the **very end** because I tried to mix Asio, async, class inheritance and multithreading. Surely I had written absolute spaghetti code laced with memory leaks.

# How it works

Here is a diagram explaining how everything connects to each other.

![discraft_diagram](https://github.com/user-attachments/assets/f4b462e6-bd87-46ab-91f7-36969165b05d)

*APP is our program.*

Note: each side of our app (client and server) commands one Discord bot.
