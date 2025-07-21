search_entries = Search
delete_entry = Delete
incognito = Incognito
clear_entries = Clear
show_qr_code = Show QR code
return_to_clipboard = Return to clipboard
qr_code_error = Error while generating the QR code
horizontal_layout = Horizontal
add_favorite = Add Favorite
remove_favorite = Remove Favorite
unique_session = Unique session
unknown_mime_types_title = Mime types

data_control = Dummy
    .title = You need to activate the data control Wayland protocol on your device
    .explanation =
        The [data control protocol](https://wayland.app/protocols/ext-data-control-v1) is required for this applet to work. It allows any privileged client to access the clipboard
        without any action from the user. It is thus somewhat insecure.
    .cosmic = 
        The protocol is disabled by default on the COSMIC desktop environment, but you can enable it with the following command: