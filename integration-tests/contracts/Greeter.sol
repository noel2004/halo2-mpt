// SPDX-License-Identifier: GPL-3.0

pragma solidity >=0.7.0 <0.9.0;

/**
 * @title Greeter
 * @dev Store & retrieve value in a variable
 */
contract Greeter {

    uint256 number;

    constructor(uint256 num) {
        number = num;
    }

    function retrieve() public view returns (uint256){
        return number;
    }

    function retrieve_failing() public view returns (uint256){
        require(false);
        return number;
    }

    function set_value(uint256 num) public{
        number = num;
    }

    function set_value_failing(uint256 num) public{
        number = num;
        require(false);
    }

    function set_value_and_receive(uint256 num) public payable {
        number = num;
    }

    function set_value_and_receive_failing(uint256 num) public payable {
        number = num;
        revert("deliberately");
    }
}

/**
 * @title DualGreeter
 * @dev Store & retrieve value in a variable, has 2 slots
 */
contract DualGreeter {

    uint256 number1;
    uint256 number2;

    constructor() {
    }

    function retrieve1() public view returns (uint256){
        return number1;
    }

    function retrieve2() public view returns (uint256){
        return number2;
    }

    function set_value1(uint256 num) public{
        number1 = num;
    }

    function set_value2(uint256 num) public{
        number2 = num;
    }
}
