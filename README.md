# Discraft  
Playing Minecraft through Discord!

> [!IMPORTANT]  
> This project is **almost** finished, I just have to get the determination to do the final plumbing and debug intricate bugs.

# The Original Idea  

I wondered: would it be possible to play Minecraft through Discord? That would require some knowledge of asynchronous programming, wouldn't it? And it's quite funny, isn't it?  

This was an original idea I initially attempted to implement in C++. However, I encountered challenges at the **very end** when trying to mix Asio, asynchronous programming, class inheritance, and multithreading. Surely, I had written absolute spaghetti code riddled with memory leaks.  

# How It Works  

Below is a diagram explaining how all components interact with one another.  

![discraft_diagram](https://github.com/user-attachments/assets/f4b462e6-bd87-46ab-91f7-36969165b05d)  

*APP is our program.*  

**Note**: Each side of our app (client and server) controls one Discord bot.  

## The Logic  

- Listen for TCP packets coming from the Minecraft client.  
  - Convert TCP packets into text suitable for Discord.  
    - Send the text to Discord.  

- Listen for Discord messages.  
  - Parse the Discord messages into bytes.  
    - Send the bytes through the socket to the Minecraft client.  

The only difference between the client side and the server side is that, on the server side, we first listen for a Discord message. Conversely, on the client side, we first wait for the Minecraft server to connect.
