https://github.com/pop-os/cosmic-epoch/issues/175

https://www.reddit.com/r/pop_os/comments/194bugz/comment/khfxyoo/?utm_source=share&utm_medium=web3x&utm_name=web3xcss&utm_term=1&utm_content=share_button

https://github.com/mohamadzoh/clipop

https://github.com/YaLTeR/wl-clipboard-rs

get keyboard data
- https://crates.io/search?q=wl-clipboard-rs
- https://crates.io/crates/arboard
- https://crates.io/crates/wl-clipboard-rs


protocol
https://wayland.app/protocols/wlr-data-control-unstable-v1


https://github.com/bugaevc/wl-clipboard/tree/master/src




deps:

```
sudo dnf install libxkbcommon-devel -y
```



DB:

stoker: mime type, data, time creation

sort by time creation
fast delete
fast search by data


key must be data

another database with key
iter must be by time creation



app thread

thread clipboard qui recois des events

dans l'app, on a une connexion qui permet de:

supprimer une entry
ajouter une entry

l'app utilisera un IndexHashMap. Elle sera initialiser au debut avec le contenu de la db.



Message {
    RemoveEntry(elem), // from the user
    AddEntry(Event), // from the keyboard thread
}


Remove {

    state.remove(elem);

    db.clone()
    async {
        db.remove(elem.id)
    }
}


// Tree<Time, Data>

// State<Data, Other>

Add {

    if let(elem) = state.insert(data, other) {
        // we need to remove it from the db
        db.remove(elem.id)
    }

    db.insert(data, other)
}

