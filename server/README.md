# procman
Very simple tool to spawn and manage local processes with a REST API and webbased UI

This tool is currently in very early stages and should not be used yet. However, the following tutorial is already working (windows only for now)

## Tutorial (Windows only)
* Start ```procman``` without any parameters. It will listen on ```127.0.0.1:3000```
* Open a browser and type ```http://localhost:3000/shell?cmd=dir```  
--> A directory listing will be shown in your browser
* Instead of ```dir``` you can use other shell commands, for example ```echo hello world```. You just need to url encode the command  
http://localhost:3000/shell?cmd=echo+hello+world!
