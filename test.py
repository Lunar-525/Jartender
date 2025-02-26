from modules import Lister
server_list = Lister.load_server_list()
print(Lister.display_servers(server_list))