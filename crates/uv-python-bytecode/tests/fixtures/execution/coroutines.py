
import asyncio

async def child():
    return 42

async def numbers():
    yield 1
    yield 2

async def main():
    iterator = numbers()
    return await child(), await anext(iterator), await anext(iterator)

async def collect(stop):
    result = []
    async for number in numbers():
        if number == stop:
            break
        result.append(number)
    else:
        result.append('done')
    return result

events = []

class AsyncManager:
    async def __aenter__(self):
        events.append('enter')
        return 42

    async def __aexit__(self, typ, value, traceback):
        events.append(typ.__name__ if typ else 'exit')
        return typ is ValueError

async def contexts():
    async with AsyncManager() as value:
        events.append(value)
    async with AsyncManager():
        raise ValueError('suppressed')
    return events

async def return_from_context():
    async with AsyncManager() as value:
        return value + 1

async def loop_contexts():
    values = []
    for index in range(3):
        async with AsyncManager() as value:
            values.append(value)
            if index == 0:
                continue
            break
    return values

async def comprehensions():
    generated = (number async for number in numbers())
    return (
        [number * 2 async for number in numbers() if number > 1],
        {number async for number in numbers()},
        {number: number * 2 async for number in numbers()},
        [number async for number in generated],
    )

def awaited_generator():
    return (await child() for _ in [0, 1])

async def consume_awaited_generator():
    return [value async for value in awaited_generator()]

print(
    asyncio.run(main()),
    asyncio.run(collect(99)),
    asyncio.run(collect(2)),
    asyncio.run(contexts()),
    asyncio.run(comprehensions()),
    asyncio.run(consume_awaited_generator()),
)
print(
    asyncio.run(return_from_context()),
    events[-2:],
    asyncio.run(loop_contexts()),
    events[-4:],
)
