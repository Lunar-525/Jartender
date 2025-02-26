from jartender import BColors

def gradient_yellow_rgb(text, offset):
    start_color = (112, 214, 255)
    end_color = (255, 112, 166)
    length = len(text)
    colored_text = ""

    offset = offset * 2

    for i, char in enumerate(text):
        factor = (i + offset) / (length + offset)
        r = int(start_color[0] + (end_color[0] - start_color[0]) * factor)
        g = int(start_color[1] + (end_color[1] - start_color[1]) * factor)
        b = int(start_color[2] + (end_color[2] - start_color[2]) * factor)

        colored_text += f"\033[38;2;{r};{g};{b}m{char}"

    return colored_text + "\033[0m"




def about():
    print(BColors.OKGREEN +        r"===========================================================================")
    print(gradient_yellow_rgb(r"     ██╗ █████╗ ██████╗ ████████╗███████╗███╗   ██╗██████╗ ███████╗██████╗ ", 0))
    print(gradient_yellow_rgb(r"     ██║██╔══██╗██╔══██╗╚══██╔══╝██╔════╝████╗  ██║██╔══██╗██╔════╝██╔══██╗", 1))
    print(gradient_yellow_rgb(r"     ██║███████║██████╔╝   ██║   █████╗  ██╔██╗ ██║██║  ██║█████╗  ██████╔╝", 2))
    print(gradient_yellow_rgb(r"██   ██║██╔══██║██╔══██╗   ██║   ██╔══╝  ██║╚██╗██║██║  ██║██╔══╝  ██╔══██╗", 3))
    print(gradient_yellow_rgb(r"╚█████╔╝██║  ██║██║  ██║   ██║   ███████╗██║ ╚████║██████╔╝███████╗██║  ██║", 4))
    print(gradient_yellow_rgb(r" ╚════╝ ╚═╝  ╚═╝╚═╝  ╚═╝   ╚═╝   ╚══════╝╚═╝  ╚═══╝╚═════╝ ╚══════╝╚═╝  ╚═╝", 5))
    print(gradient_yellow_rgb(r"https://github.com/tucaoba2333/Jartender                            @L_unar", 6))
    print(BColors.OKGREEN +        r"===========================================================================")





