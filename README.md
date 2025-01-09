# copernicus-rust

Small cli tool to list and download Copernicus Data Space Earth observation imagery.
This tool is currently a work in progress.


## Setup

Add a .env file that looks like the included template with your Copernicus Data Space Ecosystem.

Registration [here](https://documentation.dataspace.copernicus.eu/Registration.html).


## Running / Testing

Run `cargo run -- --bbox=-75.201704,39.981552,-75.114099,39.915099` while in the
project folder to test. Authentication and list operations may take some time
depending on other query parameters provided. You can see what these are with `--help`.
