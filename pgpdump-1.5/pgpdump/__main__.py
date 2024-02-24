import sys

from . import AsciiData, BinaryData


def parsefile(name):
    with open(name, 'rb') as infile:
        if name.endswith('.asc') or name.endswith('.txt'):
            data = AsciiData(infile.read())
        else:
            data = BinaryData(infile.read())

    for packet in data.packets():
        yield packet


def main():
    counter = length = 0
    for filename in sys.argv[1:]:
        for packet in parsefile(filename):
            counter += 1
            length += packet.length

    print('%d packets, length %d' % (counter, length))


if __name__ == '__main__':
    #import cProfile
    #cProfile.run('main()', 'pgpdump.profile')
    main()
