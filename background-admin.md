# Background for Server Administrators
Running a Puzzleverse server allows you to create a control a community.
It is part technical and part social. You will be responsible for how players
on your server conduct themselves when visiting other servers and other
administrators can choose to cut _all_ your players off from the network.

This guide will cover mostly the technical aspects and the tools available to
manage your community. For information on running a community, the [Run Your
Own Social](https://runyourown.social/) guide is recommended reading.

## Getting a Server
You will need:

- a Linux or Windows server that can run a PostgreSQL database.
- a domain name
- an SSL certificate

The server RAM and CPU requirements are quite modest. The server will need to
cache in-game assets, so a large amount of disk is desirable.

Using a cloud hosting provider such as AWS or Azure should be quite sensible.
Since the setup procedures for these services are somewhat different, this
guide does not cover provisioning the server. This assumes you already have a
login to a Linux terminal.

## Certificates on Ubuntu
You will need to set up an SSL certificate to allow your server to be public.
You can generate a certificate using [Let's Encrypt](https://letsencrypt.org/)
as follows. To do so, run:

    sudo apt-get install certbot
    sudo certbot certonly --standalone

You will need to create a PKCS12 file from the certificate:

    openssl pkcs12 -export -out /etc/puzzleverse/cert.pfx -inkey /etc/letsencrypt/live/${SERVER_NAME}/privkey.pem -in /etc/letsencrypt/live/${SERVER_NAME}/cert.pem -certfile /etc/letsencrypt/live/${SERVER_NAME}/fullchain.pem

This must be done when `certbot` updates the signature, so it maybe best to put this in a systemd cron job:

    sudo cp puzzleverse-pkcs12.* /lib/systemd/system
    sudo systemctl daemon-reload

This will work on all systemd-based Linux distributions, including Debian and Ubuntu.

## Certificates on Other Operating Systems
For other operating systems, follow the [Certbot
instructions](https://certbot.eff.org/instructions), selecting _None of the
above_ for the software and then using the _Yes, my web server is not currently
running on this machine_ path in the instructions.


## Server Installation on Ubuntu
These instructions are meant for a Debian or Ubuntu server. It is possible to
install on other Linux distributions or operating systems.

First, create a directory for the server configuration:

    sudo mkdir /etc/puzzleverse

You will need the [PostgrSQL](https://www.postgresql.org/) database. On Debian/Ubuntu, invoke:

    sudo apt-get install postgesql

Generate a random database password:

    DB_PASS=$(openssl rand -base64 32)
    echo $DB_PASS

Then, create a new database for Puzzleverse. On Debian/Ubuntu, invoke:

    sudo -u postgres psql -c "CREATE ROLE puzzleverse PASSWORD '"${DB_PASS}"' LOGIN; CREATE DATABASE puzzleverse OWNER puzzleverse;"

For other operating systems, connect to the database as the administrator and then, substituting `DB_PASS` with the real password, issue:

    CREATE ROLE puzzleverse PASSWORD 'DB_PASS' LOGIN;
    CREATE DATABASE puzzleverse OWNER puzzleverse;


TODO authconfig

Now, install the server itself. If you have downloaded a binary:

    sudo cp puzzleverse-server /usr/bin
    sudo cp puzzleverse.service /lib/systemd/system

On Ubuntu, you can install from a PPA:

    sudo apt-add-repository -y ppa:puzzleverse/ppa
    sudo apt-get update
    sudo apt-get install puzzleverse-server

Skip this step iIf using a systemd-based Linux distribution, including Debian, copy the default configuration as follows:

    sudo cp defaults /etc/default/puzzleverse
    ./generate-config | sudo tee /etc/default/puzzleverse
    sudo systemctl daemon-reload
    sudo systemctl enable puzzleverse
    sudo systemctl start puzzleverse

And that should be all!
    puzzleverse-server -a method:/etc/puzzleverse/auth.cfg -d postgres://puzzleverse:test@localhost/puzzleverse -j ${JWT_SECRET} --ssl /etc/puzzleverse/cert.pfx

TODO

## Managing Server Access
Much like individual players can choose who access their realms and who can
send them direct messages, the server administrator can also set access rules.
They server administrators can set:

- which players from other servers can access this server
- which players from other servers can send direct messages to players on this server
- which players on this server can update these rules

The rules are combined with the player's personal rules. So, if a remote player
is blocked at a server level, there is no way for an individual player to still
allow that player access to their realms.

## Starting the Journey
When a new player joins your server, they will only be allowed to access their
home realm. Their home realm must have a trigger to _debut_ the player deciding
that they are ready to interact with the outside world. As the administrator,
you can list possible realms for your players. This is part of the
configuration of the server and the realm must be a train-car realm.

## Train-Car Realms
The server administrator can add realm descriptions to use as train cars. The
server will choose realms the player has not played as train cars. Not all
realms can be used as train cars (they must have a link to the next car). Once
added, train cars cannot be deleted. When adding a realm, the administrator can
choose if this realm is an appropriate first realm for a player. If multiple
realms are available as first realms, the system will choose one randomly. A
realm that is marked as appropriate for a first realm can also be used for
non-first train cars.
