# rusted-empire
rust port of VMS-empire. mostly true to the original project with a couple of UI and QOL tweaks.

This is a vibecoded port of VMS-empire using gemini. the game itself is intended to be a fairly accurate reconstruction of the original empire game in rust. but as we added the rattitui TUI the temptation to improve the user interface was too strong to ignore.

As of the time of this commit the core of the game is solid. map generation is solid and this evening we completed the logic for armies. tomarrow we tackle troop transports. at this point we will just add the logic for the rest of the units and then overhaul the computer AI player logic. then I will call this port finished and fork this into a new project based on this to make it client/server multiplayer network game and start to add clients for more than just TUI interface. 
